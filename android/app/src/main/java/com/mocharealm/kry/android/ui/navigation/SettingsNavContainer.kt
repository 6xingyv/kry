package com.mocharealm.kry.android.ui.navigation

import androidx.compose.foundation.layout.Box
import androidx.compose.material3.windowsizeclass.WindowWidthSizeClass
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.navigation3.runtime.NavEntryDecorator
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.runtime.entryProvider
import androidx.navigation3.runtime.rememberDecoratedNavEntries
import androidx.navigation3.runtime.rememberNavBackStack
import androidx.navigation3.runtime.rememberSaveableStateHolderNavEntryDecorator
import androidx.navigation3.scene.SceneInfo
import androidx.navigation3.scene.SinglePaneSceneStrategy
import androidx.navigation3.scene.rememberSceneState
import androidx.navigation3.ui.NavDisplay
import androidx.navigationevent.compose.NavigationBackHandler
import androidx.navigationevent.compose.NavigationEventState
import androidx.navigationevent.compose.rememberNavigationEventState
import com.mocharealm.kry.android.settings.KeyboardProfileSpec
import com.mocharealm.kry.android.settings.KeyboardProfileStore
import com.mocharealm.kry.android.ui.screen.settings.SettingsHomeScreen
import com.mocharealm.kry.android.ui.screen.settings.fragments.CorrectionScreen
import com.mocharealm.kry.android.ui.screen.settings.fragments.LanguageScreen
import com.mocharealm.kry.android.ui.screen.settings.fragments.PreferenceScreen
import com.mocharealm.kry.android.ui.screen.settings.fragments.ThemeScreen
import kotlinx.coroutines.launch

@Composable
fun SettingsNavContainer(
    initialProfile: KeyboardProfileSpec,
    windowWidthSizeClass: WindowWidthSizeClass,
    onProfileSelected: (KeyboardProfileSpec) -> Unit,
    onExit: () -> Unit,
    onEnableKeyboard: () -> Unit,
    onSwitchKeyboard: () -> Unit,
) {
    val predictiveBackAnimationHandler = remember { AospPredictiveBackAnimation() }
    val backStack = rememberNavBackStack(SettingsRoute.Home)
    val navigator = remember(backStack) { Navigator(backStack) }
    var activeProfileId by rememberSaveable { mutableStateOf(initialProfile.id) }
    val activeProfile = KeyboardProfileStore.profileForId(activeProfileId)

    CompositionLocalProvider(LocalNavigator provides navigator) {
        var gestureState: NavigationEventState<SceneInfo<NavKey>>? = null
        val navigationScope = rememberCoroutineScope()
        val onBack: () -> Unit = {
            navigationScope.launch {
                predictiveBackAnimationHandler.onBackPressed(
                    transitionState = gestureState?.transitionState,
                    currentPageKey = navigator.current(),
                )
                if (!navigator.pop()) onExit()
            }
        }

        val entries = rememberDecoratedNavEntries(
            backStack = navigator.backStack,
            entryDecorators = listOf(
                rememberSaveableStateHolderNavEntryDecorator(),
                NavEntryDecorator(
                    onPop = { key ->
                        predictiveBackAnimationHandler.onPagePop(
                            contentPageKey = key,
                            animationScope = navigationScope,
                        )
                    },
                ) { content ->
                    with(predictiveBackAnimationHandler) {
                        Box(
                            modifier = Modifier.predictiveBackAnimationDecorator(
                                transitionState = gestureState?.transitionState,
                                contentPageKey = content.contentKey,
                                currentPageKey = navigator.current(),
                            ),
                        ) {
                            content.Content()
                        }
                    }
                },
            ),
            entryProvider = entryProvider {
                entry<SettingsRoute.Home> {
                    SettingsHomeScreen(
                        profile = activeProfile,
                        windowWidthSizeClass = windowWidthSizeClass,
                        onExit = onExit,
                        onEnableKeyboard = onEnableKeyboard,
                        onSwitchKeyboard = onSwitchKeyboard,
                    )
                }
                entry<SettingsRoute.Language> {
                    LanguageScreen(
                        selected = activeProfile,
                        windowWidthSizeClass = windowWidthSizeClass,
                        onSelect = { profile ->
                            activeProfileId = profile.id
                            onProfileSelected(profile)
                        },
                    )
                }
                entry<SettingsRoute.Preferences> {
                    PreferenceScreen(windowWidthSizeClass = windowWidthSizeClass)
                }
                entry<SettingsRoute.Correction> {
                    CorrectionScreen(windowWidthSizeClass = windowWidthSizeClass)
                }
                entry<SettingsRoute.Theme> {
                    ThemeScreen(windowWidthSizeClass = windowWidthSizeClass)
                }
            },
        )

        val sceneState = rememberSceneState(
            entries = entries,
            sceneStrategies = listOf(SinglePaneSceneStrategy()),
            sceneDecoratorStrategies = emptyList(),
            sharedTransitionScope = null,
            onBack = onBack,
        )
        val scene = sceneState.currentScene
        val currentInfo = SceneInfo(scene)
        val previousSceneInfos = sceneState.previousScenes.map { SceneInfo(it) }
        gestureState = rememberNavigationEventState(
            currentInfo = currentInfo,
            backInfo = previousSceneInfos,
        )

        NavigationBackHandler(
            state = gestureState,
            isBackEnabled = scene.previousEntries.isNotEmpty(),
            onBackCompleted = onBack,
        )

        NavDisplay(
            sceneState = sceneState,
            navigationEventState = gestureState,
            contentAlignment = Alignment.TopStart,
            sizeTransform = null,
            predictivePopTransitionSpec = { swipeEdge ->
                predictiveBackAnimationHandler.onPredictivePopTransitionSpec(this, swipeEdge)
            },
            popTransitionSpec = {
                predictiveBackAnimationHandler.onPopTransitionSpec(this)
            },
            transitionSpec = {
                predictiveBackAnimationHandler.onTransitionSpec(this)
            },
        )
    }
}
