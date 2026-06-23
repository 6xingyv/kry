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
fun CorrectionScreen(
    windowWidthSizeClass: WindowWidthSizeClass,
) {
    val navigator = LocalNavigator.current
    var autoCorrection by rememberSaveable { mutableStateOf(true) }
    var showSuggestions by rememberSaveable { mutableStateOf(true) }
    var gestureTyping by rememberSaveable { mutableStateOf(true) }
    var personalDictionary by rememberSaveable { mutableStateOf(true) }

    SettingsScaffold(
        title = "更正和建议",
        windowWidthSizeClass = windowWidthSizeClass,
        onBack = { navigator.pop() },
        largeTopBar = false,
    ) {
        item {
            SegmentedColumn(title = "候选词") {
                item {
                    SwitchSettingItem(
                        icon = R.drawable.ic_search_24px,
                        title = "显示建议",
                        description = "在候选栏显示输入建议",
                        checked = showSuggestions,
                        onCheckedChange = { showSuggestions = it },
                    )
                }
                item {
                    SwitchSettingItem(
                        icon = R.drawable.ic_translate_24px,
                        title = "自动更正",
                        description = "按空格时使用高置信候选词",
                        checked = autoCorrection,
                        onCheckedChange = { autoCorrection = it },
                    )
                }
                item {
                    SwitchSettingItem(
                        icon = R.drawable.ic_menu_book_24px,
                        title = "个人词典",
                        description = "将常用输入保存在本地词典",
                        checked = personalDictionary,
                        onCheckedChange = { personalDictionary = it },
                    )
                }
            }
        }

        item {
            SegmentedColumn(title = "滑行输入") {
                item {
                    SwitchSettingItem(
                        icon = R.drawable.ic_gif_24px,
                        title = "滑行输入",
                        description = "在字母间滑动以输入单词",
                        checked = gestureTyping,
                        onCheckedChange = { gestureTyping = it },
                    )
                }
            }
        }

        item { SettingsBottomSpacer() }
    }
}
