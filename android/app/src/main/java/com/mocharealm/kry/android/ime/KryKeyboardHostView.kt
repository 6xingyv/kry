package com.mocharealm.kry.android.ime

import android.content.Context
import android.text.Spannable
import android.text.SpannableString
import android.text.style.UnderlineSpan
import android.view.Gravity
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.widget.FrameLayout
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.platform.ComposeView
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.setViewTreeLifecycleOwner
import androidx.lifecycle.setViewTreeViewModelStoreOwner
import androidx.savedstate.SavedStateRegistryOwner
import androidx.savedstate.setViewTreeSavedStateRegistryOwner
import com.mocharealm.kry.android.nativebridge.Candidate
import com.mocharealm.kry.android.nativebridge.KryEngine
import com.mocharealm.kry.android.nativebridge.TapPoint
import com.mocharealm.kry.android.settings.KeyboardProfileStore
import com.mocharealm.kry.android.ui.ime.KryKeyboardSurface
import com.mocharealm.kry.android.ui.ime.ShiftState
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.roundToInt
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private const val KeyboardHeightDp = 340
private const val CandidateStripHeightDp = 56
private const val CaptureHeightDp = 220
private const val MaxTapPoints = 64
private const val MotionSampleBytes = 24

/** How many chars before the cursor to feed the LM as context. A char LM only needs
 *  a short window for cross-word coherence; keep it small so the InputConnection IPC
 *  stays cheap. */
private const val EditorContextChars = 64

/** Horizontal padding (dp) applied inside KeyboardRows. */
private const val KeyGridPaddingHorizontalDp = 8
/** Vertical padding (dp) applied inside KeyboardRows. */
private const val KeyGridPaddingVerticalDp = 8

class KryKeyboardHostView(
    context: Context,
    private val engine: KryEngine,
    private val inputConnectionProvider: () -> InputConnection?,
    private val canSwitchImeProvider: () -> Boolean,
    private val onOpenSettings: () -> Unit,
    private val onSwitchIme: () -> Unit,
    private val onProfileChanged: (String) -> Unit,
) : FrameLayout(context) {
    private val candidates = mutableStateOf<List<Candidate>>(emptyList())
    private val activeProfile = mutableStateOf(KeyboardProfileStore.activeProfile(context))
    private val enterKey = mutableStateOf(EnterKeySpec())
    private val canSwitchIme = mutableStateOf(false)
    private val shiftState = mutableStateOf(ShiftState.Off)
    private val symbolsMode = mutableStateOf(false)
    private val emojiMode = mutableStateOf(false)
    private val captureView: MotionCaptureView
    private val tapPoints = mutableListOf<TapPoint>()
    private var tapViewWidth = 1f
    private var tapViewHeight = 1f
    private val decodeScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var gestureDecodeJob: Job? = null
    private var decodeGeneration = 0L
    private val readingWhitespace = Regex("\\s+")

    init {
        val composeView = ComposeView(context).apply {
            // Set owners directly on the ComposeView to ensure it can find them during attachment.
            (context as? LifecycleOwner)?.let { setViewTreeLifecycleOwner(it) }
            (context as? ViewModelStoreOwner)?.let { setViewTreeViewModelStoreOwner(it) }
            (context as? SavedStateRegistryOwner)?.let { setViewTreeSavedStateRegistryOwner(it) }

            setContent {
                KryKeyboardSurface(
                    candidates = candidates.value,
                    activeProfile = activeProfile.value,
                    enterKey = enterKey.value,
                    canSwitchIme = canSwitchIme.value,
                    shiftState = shiftState.value,
                    symbolsMode = symbolsMode.value,
                    emojiMode = emojiMode.value,
                    chinesePunct = activeProfile.value.id == KeyboardProfileStore.ProfileZhQwerty,
                    onCandidate = { commitCandidate(it) },
                    onSpace = { handleSpace() },
                    onBackspace = { handleBackspace() },
                    onEnter = { handleEnter() },
                    onSwitchProfile = { switchProfile() },
                    onOpenSettings = onOpenSettings,
                    onSwitchIme = onSwitchIme,
                    onShift = { cycleShift() },
                    onSymbols = { toggleSymbols() },
                    onEmoji = { openEmojiPicker() },
                    onEmojiPicked = { text -> handleEmoji(text) },
                    onCloseEmoji = { closeEmojiPicker() },
                    onPunct = { text -> handlePunct(text) },
                    onSymbolInput = { text -> handlePunct(text) },
                )
            }
        }
        addView(
            composeView,
            LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT),
        )

        captureView = MotionCaptureView(context).apply {
            onTap = { x, y, width, height ->
                handleTap(x, y, width, height)
            }
            onGesture = { buffer, sampleCount, width, height ->
                tapPoints.clear()
                decodeGestureAsync(buffer, sampleCount, width, height)
            }
            gridInsetLeft = dpToPx(KeyGridPaddingHorizontalDp)
            gridInsetTop = dpToPx(KeyGridPaddingVerticalDp)
            gridInsetRight = dpToPx(KeyGridPaddingHorizontalDp)
            gridInsetBottom = dpToPx(KeyGridPaddingVerticalDp)
            // Shift/Backspace live in the row-2 indents (geometry x<0.15 / x>0.85,
            // bottom band). Pass those touches through to the Compose keys so they are
            // not decoded as spurious left/right-edge letters.
            passthroughGridZones = listOf(
                android.graphics.RectF(0f, 0.651f, 0.15f, 1f),
                android.graphics.RectF(0.85f, 0.651f, 1f, 1f),
            )
        }
        addView(
            captureView,
            LayoutParams(
                LayoutParams.MATCH_PARENT,
                dp(CaptureHeightDp),
                Gravity.TOP,
            ).apply {
                topMargin = dp(CandidateStripHeightDp)
            },
        )

        loadUserDict()
    }

    fun resetSession() {
        setCandidates(emptyList())
        tapPoints.clear()
        // A new input field starts on the letter layer with shift cleared.
        shiftState.value = ShiftState.Off
        symbolsMode.value = false
        emojiMode.value = false
        captureView.visibility = View.VISIBLE
        decodeGeneration += 1
        gestureDecodeJob?.cancel()
        decodeScope.launch(Dispatchers.Default) {
            engine.resetSwipe()
        }
    }

    fun refreshProfile() {
        activeProfile.value = KeyboardProfileStore.activeProfile(context)
    }

    /** The native engine finished loading. Re-decode any taps the user made while
     *  it was still loading (those returned empty), so candidates appear without a
     *  re-tap. Safe no-op if nothing is pending. */
    fun onEngineReady() {
        if (tapPoints.isNotEmpty()) decodeTapsAsync()
    }

    /** Push the editor's text-before-cursor into the engine so the LM re-ranks with
     *  the field's real surrounding context (pre-existing text, text typed by another
     *  keyboard, a cursor placed mid-document) — not just what this keyboard committed
     *  this session. Reads the InputConnection on the caller's thread (bounded, cheap)
     *  then sets the engine context off the main thread. Call on focus / cursor move. */
    fun syncEditorContext() {
        val before = inputConnectionProvider()
            ?.getTextBeforeCursor(EditorContextChars, 0)
            ?.toString()
            .orEmpty()
        decodeScope.launch(Dispatchers.Default) {
            engine.setContext(before)
        }
    }

    fun updateEditorInfo(info: EditorInfo?) {
        enterKey.value = EnterKeySpec.from(info)
        canSwitchIme.value = canSwitchImeProvider()
    }

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val exactHeight = MeasureSpec.makeMeasureSpec(dp(KeyboardHeightDp), MeasureSpec.EXACTLY)
        super.onMeasure(widthMeasureSpec, exactHeight)
    }

    override fun onDetachedFromWindow() {
        gestureDecodeJob?.cancel()
        decodeScope.cancel()
        super.onDetachedFromWindow()
    }

    private fun handleTap(x: Float, y: Float, width: Float, height: Float) {
        if (tapPoints.size >= MaxTapPoints) return
        tapViewWidth = width
        tapViewHeight = height
        tapPoints += TapPoint(x.coerceIn(0f, width), y.coerceIn(0f, height))
        decodeTapsAsync()
    }

    /** Decode taps OFF the main thread. The engine's decode is @Synchronized and,
     *  during a (background) engine reload, briefly contends with the handle swap;
     *  keeping it off the UI thread guarantees taps can never stall the UI / ANR. */
    private fun decodeTapsAsync() {
        val generation = ++decodeGeneration
        gestureDecodeJob?.cancel()
        val snapshot = tapPoints.toList()
        if (snapshot.isEmpty()) {
            setCandidates(emptyList())
            return
        }
        gestureDecodeJob = decodeScope.launch {
            val decoded = withContext(Dispatchers.Default) {
                engine.decodeTaps(snapshot, tapViewWidth, tapViewHeight)
            }
            if (generation == decodeGeneration) setCandidates(decoded)
        }
    }

    /** Update the candidate strip AND mirror the in-progress reading into the editor
     *  as underlined composing text. One place keeps the strip and the field in sync. */
    private fun setCandidates(list: List<Candidate>) {
        candidates.value = list
        refreshComposingText()
    }

    /** Show the top candidate's reading in the target field as underlined composing
     *  text, pinyin syllables joined by `'` (e.g. ni hao → "ni'hao"), so the user can
     *  read the pinyin they're typing. Cleared (composing finished) when there's no
     *  pending input. Committing a candidate replaces this region with the real text. */
    private fun refreshComposingText() {
        val inputConnection = inputConnectionProvider() ?: return
        val reading = candidates.value.firstOrNull()?.reading?.trim().orEmpty()
        if (reading.isEmpty()) {
            inputConnection.finishComposingText()
            return
        }
        val pinyin = reading.replace(readingWhitespace, "'")
        val underlined = SpannableString(pinyin).apply {
            setSpan(UnderlineSpan(), 0, length, Spannable.SPAN_EXCLUSIVE_EXCLUSIVE)
        }
        inputConnection.setComposingText(underlined, 1)
    }

    private fun handleSpace() {
        if (!commitDefaultCandidate()) {
            inputConnectionProvider()?.commitText(" ", 1)
        }
    }

    private fun handlePunct(text: String) {
        commitDefaultCandidate() // flush any pending word first
        inputConnectionProvider()?.commitText(text, 1)
        tapPoints.clear()
        setCandidates(emptyList())
    }

    private fun handleEmoji(text: String) {
        commitDefaultCandidate()
        inputConnectionProvider()?.commitText(text, 1)
        tapPoints.clear()
        setCandidates(emptyList())
    }

    private fun handleBackspace() {
        if (tapPoints.isNotEmpty()) {
            tapPoints.removeAt(tapPoints.lastIndex)
            decodeTapsAsync()
            return
        }
        if (candidates.value.isNotEmpty()) {
            setCandidates(emptyList())
            return
        }
        val inputConnection = inputConnectionProvider() ?: return
        if (!inputConnection.deleteSurroundingTextInCodePoints(1, 0)) {
            inputConnection.deleteSurroundingText(1, 0)
        }
    }

    private fun handleEnter() {
        commitDefaultCandidate()
        val inputConnection = inputConnectionProvider() ?: return
        val spec = enterKey.value
        if (spec.performEditorAction) {
            if (!inputConnection.performEditorAction(spec.actionId)) {
                inputConnection.commitText("\n", 1)
            }
        } else {
            inputConnection.commitText("\n", 1)
        }
    }

    private fun switchProfile() {
        val next = KeyboardProfileStore.nextProfile(context)
        activeProfile.value = next
        resetSession()
        onProfileChanged(next.id)
    }

    private fun commitDefaultCandidate(): Boolean {
        val candidate = candidates.value.firstOrNull() ?: return false
        commitCandidate(candidate)
        return true
    }

    private fun commitCandidate(candidate: Candidate) {
        val text = candidate.text
        val reading = candidate.reading
        // Shift only affects the committed glyph case (English); the user dictionary
        // still learns the canonical lowercase text/reading.
        inputConnectionProvider()?.commitText(applyShift(text), 1)
        consumeOneShotShift()
        decodeScope.launch(Dispatchers.Default) {
            engine.acceptCandidate(text, reading)
            persistUserDict()
        }
        setCandidates(emptyList())
        tapPoints.clear()
    }

    private fun cycleShift() {
        // Single key cycles through all three states so caps-lock is reachable:
        // Off → Shifted (one-shot) → Locked (caps lock) → Off.
        shiftState.value = when (shiftState.value) {
            ShiftState.Off -> ShiftState.Shifted
            ShiftState.Shifted -> ShiftState.Locked
            ShiftState.Locked -> ShiftState.Off
        }
    }

    private fun applyShift(text: String): String = when (shiftState.value) {
        ShiftState.Off -> text
        ShiftState.Shifted -> text.replaceFirstChar { it.uppercase() }
        ShiftState.Locked -> text.uppercase()
    }

    /** A one-shot Shifted latch reverts to Off after a single commit; Locked stays. */
    private fun consumeOneShotShift() {
        if (shiftState.value == ShiftState.Shifted) shiftState.value = ShiftState.Off
    }

    private fun toggleSymbols() {
        val next = !symbolsMode.value
        symbolsMode.value = next
        // The capture overlay must not intercept while symbols are shown; the Compose
        // symbol keys handle taps directly.
        captureView.visibility = if (next) View.GONE else View.VISIBLE
        if (next) {
            tapPoints.clear()
            setCandidates(emptyList())
        }
    }

    private fun openEmojiPicker() {
        symbolsMode.value = false
        emojiMode.value = true
        tapPoints.clear()
        setCandidates(emptyList())
        captureView.visibility = View.GONE
    }

    private fun closeEmojiPicker() {
        emojiMode.value = false
        captureView.visibility = View.VISIBLE
    }

    private val userDictFile by lazy { java.io.File(context.filesDir, "user_dict.tsv") }

    /** Load the persisted adaptive user dictionary once, off the main thread. */
    private fun loadUserDict() {
        decodeScope.launch(Dispatchers.Default) {
            runCatching {
                if (userDictFile.exists()) engine.importUserDict(userDictFile.readText())
            }
        }
    }

    /** Write the learned user dictionary back to disk (called after commits). */
    private fun persistUserDict() {
        runCatching { userDictFile.writeText(engine.exportUserDict()) }
    }

    private fun decodeGestureAsync(
        buffer: ByteBuffer,
        sampleCount: Int,
        width: Float,
        height: Float,
    ) {
        if (sampleCount <= 0) return
        val snapshot = snapshotGestureBuffer(buffer, sampleCount)
        if (snapshot.sampleCount <= 0) return
        val generation = ++decodeGeneration
        gestureDecodeJob?.cancel()
        setCandidates(emptyList())
        gestureDecodeJob = decodeScope.launch {
            val decoded = withContext(Dispatchers.Default) {
                engine.decodeGesture(snapshot.buffer, snapshot.sampleCount, width, height)
            }
            if (generation == decodeGeneration) {
                setCandidates(decoded)
            }
        }
    }

    private fun snapshotGestureBuffer(buffer: ByteBuffer, sampleCount: Int): GestureBufferSnapshot {
        val byteCount = (sampleCount * MotionSampleBytes).coerceAtMost(buffer.capacity())
        val source = buffer.duplicate().order(ByteOrder.nativeOrder())
        source.position(0)
        source.limit(byteCount)
        val copy = ByteBuffer
            .allocateDirect(source.remaining())
            .order(ByteOrder.nativeOrder())
            .apply {
                put(source)
                position(0)
            }
        return GestureBufferSnapshot(copy, byteCount / MotionSampleBytes)
    }

    private fun dp(value: Int): Int {
        return (value * resources.displayMetrics.density).roundToInt()
    }

    private fun dpToPx(value: Int): Float {
        return value * resources.displayMetrics.density
    }
}

private data class GestureBufferSnapshot(
    val buffer: ByteBuffer,
    val sampleCount: Int,
)

data class EnterKeySpec(
    val actionId: Int = EditorInfo.IME_ACTION_NONE,
    val performEditorAction: Boolean = false,
) {
    companion object {
        fun from(info: EditorInfo?): EnterKeySpec {
            val imeOptions = info?.imeOptions ?: EditorInfo.IME_ACTION_NONE
            val actionId = imeOptions and EditorInfo.IME_MASK_ACTION
            val noEnterAction = (imeOptions and EditorInfo.IME_FLAG_NO_ENTER_ACTION) != 0
            return when {
                noEnterAction -> EnterKeySpec()
                actionId == EditorInfo.IME_ACTION_GO -> EnterKeySpec(actionId, true)
                actionId == EditorInfo.IME_ACTION_SEARCH -> EnterKeySpec(actionId, true)
                actionId == EditorInfo.IME_ACTION_SEND -> EnterKeySpec(actionId, true)
                actionId == EditorInfo.IME_ACTION_NEXT -> EnterKeySpec(actionId, true)
                actionId == EditorInfo.IME_ACTION_DONE -> EnterKeySpec(actionId, true)
                actionId == EditorInfo.IME_ACTION_PREVIOUS -> EnterKeySpec(actionId, true)
                else -> EnterKeySpec()
            }
        }
    }
}
