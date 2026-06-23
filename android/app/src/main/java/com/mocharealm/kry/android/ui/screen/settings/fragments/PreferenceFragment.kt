package com.mocharealm.kry.android.ui.screen.settings.fragments

import androidx.compose.material3.windowsizeclass.WindowWidthSizeClass
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.res.stringResource
import com.mocharealm.kry.android.R
import com.mocharealm.kry.android.ui.navigation.LocalNavigator
import com.mocharealm.kry.android.ui.screen.settings.SettingsScaffold
import com.mocharealm.kry.android.ui.screen.settings.components.SegmentedColumn
import com.mocharealm.kry.android.ui.screen.settings.components.SettingsBottomSpacer
import com.mocharealm.kry.android.ui.screen.settings.components.SwitchSettingItem

@Composable
fun PreferenceScreen(
    windowWidthSizeClass: WindowWidthSizeClass,
) {
    val navigator = LocalNavigator.current
    var keyPreview by rememberSaveable { mutableStateOf(true) }
    var hapticFeedback by rememberSaveable { mutableStateOf(true) }
    var soundFeedback by rememberSaveable { mutableStateOf(false) }
    var oneHandedMode by rememberSaveable { mutableStateOf(false) }

    SettingsScaffold(
        title = stringResource(R.string.settings_preferences_title),
        windowWidthSizeClass = windowWidthSizeClass,
        onBack = { navigator.pop() },
        largeTopBar = false,
    ) {
        item {
            SegmentedColumn(title = stringResource(R.string.settings_section_keys)) {
                item {
                    SwitchSettingItem(
                        icon = R.drawable.ic_preview_24px,
                        title = stringResource(R.string.settings_key_preview_title),
                        description = stringResource(R.string.settings_key_preview_description),
                        checked = keyPreview,
                        onCheckedChange = { keyPreview = it },
                    )
                }
                item {
                    SwitchSettingItem(
                        icon = R.drawable.ic_vibration_24px,
                        title = stringResource(R.string.settings_haptic_feedback_title),
                        description = stringResource(R.string.settings_haptic_feedback_description),
                        checked = hapticFeedback,
                        onCheckedChange = { hapticFeedback = it },
                    )
                }
                item {
                    SwitchSettingItem(
                        icon = R.drawable.ic_volume_up_24px,
                        title = stringResource(R.string.settings_sound_feedback_title),
                        description = stringResource(R.string.settings_sound_feedback_description),
                        checked = soundFeedback,
                        onCheckedChange = { soundFeedback = it },
                    )
                }
            }
        }

        item {
            SegmentedColumn(title = stringResource(R.string.settings_section_layout)) {
                item {
                    SwitchSettingItem(
                        icon = R.drawable.ic_swipe_24px,
                        title = stringResource(R.string.settings_one_handed_mode_title),
                        description = stringResource(R.string.settings_one_handed_mode_description),
                        checked = oneHandedMode,
                        onCheckedChange = { oneHandedMode = it },
                    )
                }
            }
        }

        item { SettingsBottomSpacer() }
    }
}
