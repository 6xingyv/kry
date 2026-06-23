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
fun ThemeScreen(
    windowWidthSizeClass: WindowWidthSizeClass,
) {
    val navigator = LocalNavigator.current
    var dynamicColor by rememberSaveable { mutableStateOf(true) }
    var highContrastKeys by rememberSaveable { mutableStateOf(false) }

    SettingsScaffold(
        title = stringResource(R.string.settings_theme_title),
        windowWidthSizeClass = windowWidthSizeClass,
        onBack = { navigator.pop() },
        largeTopBar = false,
    ) {
        item {
            SegmentedColumn(title = stringResource(R.string.settings_section_color)) {
                item {
                    SwitchSettingItem(
                        icon = R.drawable.ic_palette_24px,
                        title = stringResource(R.string.settings_dynamic_color_title),
                        description = stringResource(R.string.settings_dynamic_color_description),
                        checked = dynamicColor,
                        onCheckedChange = { dynamicColor = it },
                    )
                }
                item {
                    SwitchSettingItem(
                        icon = R.drawable.ic_contrast_24px,
                        title = stringResource(R.string.settings_high_contrast_keys_title),
                        description = stringResource(R.string.settings_high_contrast_keys_description),
                        checked = highContrastKeys,
                        onCheckedChange = { highContrastKeys = it },
                    )
                }
            }
        }

        item { SettingsBottomSpacer() }
    }
}
