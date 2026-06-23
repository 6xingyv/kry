package com.mocharealm.kry.android.ui.screen.settings.fragments

import androidx.compose.material3.windowsizeclass.WindowWidthSizeClass
import androidx.compose.runtime.Composable
import com.mocharealm.kry.android.settings.KeyboardProfileSpec
import com.mocharealm.kry.android.settings.KeyboardProfileStore
import com.mocharealm.kry.android.ui.navigation.LocalNavigator
import com.mocharealm.kry.android.ui.screen.settings.SettingsScaffold
import com.mocharealm.kry.android.ui.screen.settings.components.RadioSettingItem
import com.mocharealm.kry.android.ui.screen.settings.components.SegmentedColumn
import com.mocharealm.kry.android.ui.screen.settings.components.SettingsBottomSpacer

@Composable
fun LanguageScreen(
    selected: KeyboardProfileSpec,
    windowWidthSizeClass: WindowWidthSizeClass,
    onSelect: (KeyboardProfileSpec) -> Unit,
) {
    val navigator = LocalNavigator.current

    SettingsScaffold(
        title = "语言",
        windowWidthSizeClass = windowWidthSizeClass,
        onBack = { navigator.pop() },
        largeTopBar = false,
    ) {
        item {
            SegmentedColumn {
                KeyboardProfileStore.profiles.forEach { profile ->
                    item {
                        RadioSettingItem(
                            title = profile.title,
                            description = profile.subtitle,
                            selected = profile.id == selected.id,
                            onClick = { onSelect(profile) },
                        )
                    }
                }
            }
        }
        item { SettingsBottomSpacer() }
    }
}
