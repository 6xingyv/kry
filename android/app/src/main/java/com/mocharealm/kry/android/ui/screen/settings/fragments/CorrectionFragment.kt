package com.mocharealm.kry.android.ui.screen.settings.fragments

import androidx.compose.material3.windowsizeclass.WindowWidthSizeClass
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.res.stringResource
import com.mocharealm.kry.android.R
import com.mocharealm.kry.android.ui.navigation.LocalNavigator
import com.mocharealm.kry.android.ui.screen.settings.SettingsScaffold
import com.mocharealm.kry.android.ui.screen.settings.components.SegmentedColumn
import com.mocharealm.kry.android.ui.screen.settings.components.SettingsBottomSpacer
import com.mocharealm.kry.android.ui.screen.settings.components.SwitchSettingItem

@Composable
fun CorrectionScreen(
    windowWidthSizeClass: WindowWidthSizeClass,
) {
    val navigator = LocalNavigator.current
    var autoCorrection by rememberSaveable { mutableStateOf(true) }
    var showSuggestions by rememberSaveable { mutableStateOf(true) }
    var gestureTyping by rememberSaveable { mutableStateOf(true) }
    var personalDictionary by rememberSaveable { mutableStateOf(true) }

    SettingsScaffold(
        title = stringResource(R.string.settings_correction_title),
        windowWidthSizeClass = windowWidthSizeClass,
        onBack = { navigator.pop() },
        largeTopBar = false,
    ) {
        item {
            SegmentedColumn(title = stringResource(R.string.settings_section_candidates)) {
                item {
                    SwitchSettingItem(
                        icon = R.drawable.ic_lightbulb_24px,
                        title = stringResource(R.string.settings_show_suggestions_title),
                        description = stringResource(R.string.settings_show_suggestions_description),
                        checked = showSuggestions,
                        onCheckedChange = { showSuggestions = it },
                    )
                }
                item {
                    SwitchSettingItem(
                        icon = R.drawable.ic_spellcheck_24px,
                        title = stringResource(R.string.settings_auto_correction_title),
                        description = stringResource(R.string.settings_auto_correction_description),
                        checked = autoCorrection,
                        onCheckedChange = { autoCorrection = it },
                    )
                }
                item {
                    SwitchSettingItem(
                        icon = R.drawable.ic_menu_book_24px,
                        title = stringResource(R.string.settings_personal_dictionary_title),
                        description = stringResource(R.string.settings_personal_dictionary_description),
                        checked = personalDictionary,
                        onCheckedChange = { personalDictionary = it },
                    )
                }
            }
        }

        item {
            SegmentedColumn(title = stringResource(R.string.settings_section_gesture_typing)) {
                item {
                    SwitchSettingItem(
                        icon = R.drawable.ic_gesture_24px,
                        title = stringResource(R.string.settings_gesture_typing_title),
                        description = stringResource(R.string.settings_gesture_typing_description),
                        checked = gestureTyping,
                        onCheckedChange = { gestureTyping = it },
                    )
                }
            }
        }

        item { SettingsBottomSpacer() }
    }
}
