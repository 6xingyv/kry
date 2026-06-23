package com.mocharealm.kry.android.ui.screen.settings.fragments

import androidx.compose.material3.windowsizeclass.WindowWidthSizeClass
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.mutableStateOf
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
        title = "偏好设置",
        windowWidthSizeClass = windowWidthSizeClass,
        onBack = { navigator.pop() },
        largeTopBar = false,
    ) {
        item {
            SegmentedColumn(title = "按键") {
                item {
                    SwitchSettingItem(
                        icon = R.drawable.ic_text_edit_24px,
                        title = "按键预览",
                        description = "输入时显示按键弹出预览",
                        checked = keyPreview,
                        onCheckedChange = { keyPreview = it },
                    )
                }
                item {
                    SwitchSettingItem(
                        icon = R.drawable.ic_mood_24px,
                        title = "触感反馈",
                        description = "按键时使用系统触感反馈",
                        checked = hapticFeedback,
                        onCheckedChange = { hapticFeedback = it },
                    )
                }
                item {
                    SwitchSettingItem(
                        icon = R.drawable.ic_mic_24px,
                        title = "按键音效",
                        description = "按键时播放系统按键音",
                        checked = soundFeedback,
                        onCheckedChange = { soundFeedback = it },
                    )
                }
            }
        }

        item {
            SegmentedColumn(title = "布局") {
                item {
                    SwitchSettingItem(
                        icon = R.drawable.ic_grid_view_24px,
                        title = "单手模式",
                        description = "压缩键盘宽度以便单手输入",
                        checked = oneHandedMode,
                        onCheckedChange = { oneHandedMode = it },
                    )
                }
            }
        }

        item { SettingsBottomSpacer() }
    }
}
