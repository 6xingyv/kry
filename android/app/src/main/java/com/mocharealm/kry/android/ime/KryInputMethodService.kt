package com.mocharealm.kry.android.ime

import android.content.Intent
import android.view.View
import android.view.Window
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodSubtype
import android.inputmethodservice.InputMethodService
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.setViewTreeLifecycleOwner
import androidx.lifecycle.setViewTreeViewModelStoreOwner
import androidx.savedstate.SavedStateRegistry
import androidx.savedstate.SavedStateRegistryController
import androidx.savedstate.SavedStateRegistryOwner
import androidx.savedstate.setViewTreeSavedStateRegistryOwner
import com.mocharealm.kry.android.MainActivity
import com.mocharealm.kry.android.nativebridge.KryEngine
import com.mocharealm.kry.android.settings.KeyboardProfileStore
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class KryInputMethodService : InputMethodService(),
    LifecycleOwner, ViewModelStoreOwner, SavedStateRegistryOwner {

    private val engine = KryEngine()
    private var hostView: KryKeyboardHostView? = null

    private val lifecycleRegistry = LifecycleRegistry(this)
    override val lifecycle: Lifecycle get() = lifecycleRegistry

    private val store = ViewModelStore()
    override val viewModelStore: ViewModelStore get() = store

    private val savedStateRegistryController = SavedStateRegistryController.create(this)
    override val savedStateRegistry: SavedStateRegistry get() = savedStateRegistryController.savedStateRegistry

    override fun onCreate() {
        super.onCreate()
        savedStateRegistryController.performRestore(null)
        lifecycleRegistry.handleLifecycleEvent(Lifecycle.Event.ON_CREATE)
        reloadEngine(KeyboardProfileStore.activeProfileId(this))
    }

    // Always show the soft keyboard, even when the system reports a hardware keyboard
    // (Android emulators default to one; also Bluetooth keyboards, foldables, DeX).
    // Without this, InputMethodService suppresses the input view when a hard keyboard
    // is present, so onCreateInputView() is never called and only an empty placeholder
    // window appears.
    override fun onEvaluateInputViewShown(): Boolean = true

    // The keyboard is a normal docked surface, never a fullscreen extract editor.
    override fun onEvaluateFullscreenMode(): Boolean = false

    override fun onConfigureWindow(win: Window?, isFullscreen: Boolean, isExtractView: Boolean) {
        super.onConfigureWindow(win, isFullscreen, isExtractView)
        win?.decorView?.let { decorView ->
            decorView.setViewTreeLifecycleOwner(this)
            decorView.setViewTreeViewModelStoreOwner(this)
            decorView.setViewTreeSavedStateRegistryOwner(this)
        }
    }

    override fun onCreateInputView(): View {
        val hostView = KryKeyboardHostView(
            context = this,
            engine = engine,
            inputConnectionProvider = { currentInputConnection },
            canSwitchImeProvider = { shouldOfferSwitchingToNextInputMethod() },
            onOpenSettings = { openSettings() },
            onSwitchIme = { switchToNextInputMethod(false) },
            onProfileChanged = { profileId -> reloadEngine(profileId) },
        )

        hostView.setViewTreeLifecycleOwner(this)
        hostView.setViewTreeViewModelStoreOwner(this)
        hostView.setViewTreeSavedStateRegistryOwner(this)

        return hostView.also { this.hostView = it }
    }

    override fun onStartInputView(info: EditorInfo?, restarting: Boolean) {
        super.onStartInputView(info, restarting)
        lifecycleRegistry.handleLifecycleEvent(Lifecycle.Event.ON_START)
        lifecycleRegistry.handleLifecycleEvent(Lifecycle.Event.ON_RESUME)
        hostView?.updateEditorInfo(info)
        applyProfileFromSettings()
        hostView?.syncEditorContext()
    }

    override fun onUpdateSelection(
        oldSelStart: Int,
        oldSelEnd: Int,
        newSelStart: Int,
        newSelEnd: Int,
        candidatesStart: Int,
        candidatesEnd: Int,
    ) {
        super.onUpdateSelection(
            oldSelStart, oldSelEnd, newSelStart, newSelEnd, candidatesStart, candidatesEnd,
        )
        // Cursor moved (incl. right after our own commit) — refresh the LM context from
        // the field's real text. Skip while a composing region is active (candidatesStart
        // >= 0): the field then holds the in-progress pinyin, which getTextBeforeCursor
        // would otherwise feed to the LM as bogus context. After commit/finish the
        // composing region clears and we resync with the now-committed text.
        if (candidatesStart < 0) hostView?.syncEditorContext()
    }

    override fun onFinishInputView(finishingInput: Boolean) {
        lifecycleRegistry.handleLifecycleEvent(Lifecycle.Event.ON_PAUSE)
        lifecycleRegistry.handleLifecycleEvent(Lifecycle.Event.ON_STOP)
        super.onFinishInputView(finishingInput)
    }

    override fun onStartInput(attribute: EditorInfo?, restarting: Boolean) {
        super.onStartInput(attribute, restarting)
        if (!restarting) hostView?.resetSession()
        hostView?.updateEditorInfo(attribute)
    }

    override fun onCurrentInputMethodSubtypeChanged(newSubtype: InputMethodSubtype?) {
        super.onCurrentInputMethodSubtypeChanged(newSubtype)
        val locale = subtypeLocale(newSubtype)
        val profileId = if (locale.startsWith("en")) {
            KeyboardProfileStore.ProfileEnQwerty
        } else {
            KeyboardProfileStore.ProfileZhQwerty
        }
        KeyboardProfileStore.setActiveProfile(this, profileId)
        hostView?.refreshProfile()
        hostView?.resetSession()
        reloadEngine(profileId)
    }

    override fun onDestroy() {
        lifecycleRegistry.handleLifecycleEvent(Lifecycle.Event.ON_DESTROY)
        hostView = null
        engine.close()
        store.clear()
        super.onDestroy()
    }

    private fun applyProfileFromSettings() {
        val profileId = KeyboardProfileStore.activeProfileId(this)
        hostView?.refreshProfile()
        if (!engine.isReady || engine.activeProfileId != profileId) {
            hostView?.resetSession()
            reloadEngine(profileId)
        }
    }

    private fun reloadEngine(profileId: String) {
        // initialize() is idempotent + single-flight (skips if already loaded /
        // another load is running), so calling this from several lifecycle
        // callbacks is cheap; only one heavy load actually runs.
        lifecycleScope.launch(Dispatchers.IO) {
            engine.initialize(this@KryInputMethodService, profileId)
            // A profile switch may have raced the load; reconcile to settings.
            val settingsProfile = KeyboardProfileStore.profileForId(
                KeyboardProfileStore.activeProfileId(this@KryInputMethodService),
            ).id
            if (engine.isReady && engine.activeProfileId != settingsProfile) {
                engine.initialize(this@KryInputMethodService, settingsProfile)
            }
            // Engine is usable now (decode works, no LM re-rank yet).
            withContext(Dispatchers.Main) { hostView?.onEngineReady() }
            // Install the heavy LM in the background; re-rank turns on when ready.
            engine.loadLm(this@KryInputMethodService)
            withContext(Dispatchers.Main) { hostView?.onEngineReady() }
        }
    }

    private fun openSettings() {
        val intent = Intent(this, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        startActivity(intent)
    }

    @Suppress("DEPRECATION")
    private fun subtypeLocale(subtype: InputMethodSubtype?): String {
        return subtype?.locale.orEmpty()
    }
}
