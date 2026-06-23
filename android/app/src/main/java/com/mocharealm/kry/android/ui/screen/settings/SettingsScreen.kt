package com.mocharealm.kry.android.ui.screen.settings

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LargeFlexibleTopAppBar
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.TopAppBarDefaults.exitUntilCollapsedScrollBehavior
import androidx.compose.material3.TopAppBarDefaults.topAppBarColors
import androidx.compose.material3.rememberTopAppBarState
import androidx.compose.material3.windowsizeclass.WindowWidthSizeClass
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.mocharealm.kry.android.R
import com.mocharealm.kry.android.settings.KeyboardProfileSpec
import com.mocharealm.kry.android.ui.navigation.LocalNavigator
import com.mocharealm.kry.android.ui.navigation.SettingsRoute
import com.mocharealm.kry.android.ui.screen.settings.components.NavigationSettingItem
import com.mocharealm.kry.android.ui.screen.settings.components.SegmentedColumn
import com.mocharealm.kry.android.ui.screen.settings.components.SettingsBottomSpacer

internal object SettingsDimens {
    val compactHorizontalPadding = 4.dp
    val mediumHorizontalPadding = 8.dp
    val expandedHorizontalPadding = 16.dp
    val verticalPadding = 4.dp
}

@Composable
fun SettingsHomeScreen(
    profile: KeyboardProfileSpec,
    windowWidthSizeClass: WindowWidthSizeClass,
    onExit: () -> Unit,
    onEnableKeyboard: () -> Unit,
    onSwitchKeyboard: () -> Unit,
) {
    val navigator = LocalNavigator.current

    SettingsScaffold(
        title = stringResource(R.string.settings_title),
        windowWidthSizeClass = windowWidthSizeClass,
        onBack = onExit,
        largeTopBar = true,
    ) {
        item {
            SegmentedColumn(title = stringResource(R.string.settings_section_input)) {
                item {
                    NavigationSettingItem(
                        icon = R.drawable.ic_language_24px,
                        title = stringResource(R.string.settings_language_title),
                        description = stringResource(profile.titleRes),
                        onClick = { navigator.push(SettingsRoute.Language) },
                    )
                }
                item {
                    NavigationSettingItem(
                        icon = R.drawable.ic_tune_24px,
                        title = stringResource(R.string.settings_preferences_title),
                        description = stringResource(R.string.settings_preferences_description),
                        onClick = { navigator.push(SettingsRoute.Preferences) },
                    )
                }
                item {
                    NavigationSettingItem(
                        icon = R.drawable.ic_spellcheck_24px,
                        title = stringResource(R.string.settings_correction_title),
                        description = stringResource(R.string.settings_correction_description),
                        onClick = { navigator.push(SettingsRoute.Correction) },
                    )
                }
            }
        }

        item {
            SegmentedColumn(title = stringResource(R.string.settings_section_appearance)) {
                item {
                    NavigationSettingItem(
                        icon = R.drawable.ic_palette_24px,
                        title = stringResource(R.string.settings_theme_title),
                        description = stringResource(R.string.settings_theme_description),
                        onClick = { navigator.push(SettingsRoute.Theme) },
                    )
                }
            }
        }

        item {
            SegmentedColumn(title = stringResource(R.string.settings_section_system)) {
                item {
                    NavigationSettingItem(
                        icon = R.drawable.ic_keyboard_24px,
                        title = stringResource(R.string.settings_enable_keyboard_title),
                        description = stringResource(R.string.settings_enable_keyboard_description),
                        onClick = onEnableKeyboard,
                    )
                }
                item {
                    NavigationSettingItem(
                        icon = R.drawable.ic_keyboard_tab_24px,
                        title = stringResource(R.string.settings_switch_keyboard_title),
                        description = stringResource(R.string.settings_switch_keyboard_description),
                        onClick = onSwitchKeyboard,
                    )
                }
            }
        }

        item { SettingsBottomSpacer() }
    }
}

@OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)
@Composable
internal fun SettingsScaffold(
    title: String,
    windowWidthSizeClass: WindowWidthSizeClass,
    onBack: () -> Unit,
    largeTopBar: Boolean,
    content: LazyListScope.() -> Unit,
) {
    val scrollBehavior = exitUntilCollapsedScrollBehavior(rememberTopAppBarState())
    val appBarColors = topAppBarColors(
        containerColor = MaterialTheme.colorScheme.surfaceContainer,
        scrolledContainerColor = MaterialTheme.colorScheme.surfaceContainer,
        titleContentColor = MaterialTheme.colorScheme.onBackground,
    )

    Scaffold(
        modifier = Modifier
            .fillMaxSize()
            .nestedScroll(scrollBehavior.nestedScrollConnection),
        containerColor = MaterialTheme.colorScheme.surfaceContainer,
        contentWindowInsets = WindowInsets.safeDrawing,
        topBar = {
            if (largeTopBar) {
                LargeFlexibleTopAppBar(
                    title = { Text(title) },
                    navigationIcon = { BackButton(onBack = onBack) },
                    colors = appBarColors,
                    scrollBehavior = scrollBehavior,
                )
            } else {
                TopAppBar(
                    title = { Text(title) },
                    navigationIcon = { BackButton(onBack = onBack) },
                    colors = appBarColors,
                )
            }
        },
    ) { innerPadding ->
        Box(
            modifier = Modifier
                .padding(innerPadding)
                .fillMaxSize(),
        ) {
            LazyColumn(
                modifier = Modifier
                    .align(Alignment.TopCenter)
                    .fillMaxWidth(),
                contentPadding = PaddingValues(
                    horizontal = settingsHorizontalPadding(windowWidthSizeClass),
                    vertical = SettingsDimens.verticalPadding,
                ),
                content = content,
            )
        }
    }
}

@Composable
private fun BackButton(onBack: () -> Unit) {
    IconButton(onClick = onBack) {
        Icon(
            painter = painterResource(R.drawable.ic_arrow_back_24px),
            contentDescription = stringResource(R.string.content_description_back),
        )
    }
}

private fun settingsHorizontalPadding(windowWidthSizeClass: WindowWidthSizeClass) =
    when (windowWidthSizeClass) {
        WindowWidthSizeClass.Compact -> SettingsDimens.compactHorizontalPadding
        WindowWidthSizeClass.Medium -> SettingsDimens.mediumHorizontalPadding
        else -> SettingsDimens.expandedHorizontalPadding
    }
