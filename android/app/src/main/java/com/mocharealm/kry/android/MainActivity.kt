package com.mocharealm.kry.android

import android.content.Intent
import android.os.Bundle
import android.provider.Settings
import android.view.inputmethod.InputMethodManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.material3.windowsizeclass.ExperimentalMaterial3WindowSizeClassApi
import androidx.compose.material3.windowsizeclass.calculateWindowSizeClass
import com.mocharealm.kry.android.settings.KeyboardProfileStore
import com.mocharealm.kry.android.ui.navigation.SettingsNavContainer
import com.mocharealm.kry.android.ui.theme.KryTheme

class MainActivity : ComponentActivity() {
    @OptIn(ExperimentalMaterial3WindowSizeClassApi::class)
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            val windowSizeClass = calculateWindowSizeClass(this)
            KryTheme {
                SettingsNavContainer(
                    initialProfile = KeyboardProfileStore.activeProfile(this),
                    windowWidthSizeClass = windowSizeClass.widthSizeClass,
                    onProfileSelected = { KeyboardProfileStore.setActiveProfile(this, it.id) },
                    onExit = { finish() },
                    onEnableKeyboard = {
                        startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))
                    },
                    onSwitchKeyboard = {
                        (getSystemService(INPUT_METHOD_SERVICE) as InputMethodManager)
                            .showInputMethodPicker()
                    },
                )
            }
        }
    }
}
