package com.mocharealm.kry.android.ui.screen.settings.fragments

import androidx.compose.material3.windowsizeclass.WindowWidthSizeClass
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import com.mocharealm.kry.android.R
import com.mocharealm.kry.android.ui.navigation.LocalNavigator
import com.mocharealm.kry.android.ui.screen.settings.SettingsScaffold
import com.mocharealm.kry.android.ui.screen.settings.components.SegmentedColumn
import com.mocharealm.kry.android.ui.screen.settings.components.SettingsBottomSpacer
import com.mocharealm.kry.android.ui.screen.settings.components.SwitchSettingItem

@Composable
fun ThemeScreen(
    windowWidthSizeClass: WindowWidthSizeClass,
) {
    val navigator = LocalNavigator.current
    var dynamicColor by rememberSaveable { mutableStateOf(true) }
    var highContrastKeys by rememberSaveable { mutableStateOf(false) }

    SettingsScaffold(
        title = "主题",
        windowWidthSizeClass = windowWidthSizeClass,
        onBack = { navigator.pop() },
        largeTopBar = false,
    ) {
        item {
            SegmentedColumn(title = "颜色") {
                item {
                    SwitchSettingItem(
                        icon = R.drawable.ic_palette_24px,
                        title = "动态颜色",
                        description = "跟随系统 Material You 色彩",
                        checked = dynamicColor,
                        onCheckedChange = { dynamicColor = it },
                    )
                }
                item {
                    SwitchSettingItem(
                        icon = R.drawable.ic_grid_view_24px,
                        title = "高对比按键",
                        description = "提高键盘键帽和文字对比度",
                        checked = highContrastKeys,
                        onCheckedChange = { highContrastKeys = it },
                    )
                }
            }
        }

        item { SettingsBottomSpacer() }
    }
}
