package com.mocharealm.kry.android.ui.navigation

import androidx.navigation3.runtime.NavKey
import kotlinx.serialization.Serializable

sealed interface SettingsRoute : NavKey {
    @Serializable
    data object Home : SettingsRoute

    @Serializable
    data object Language : SettingsRoute

    @Serializable
    data object Preferences : SettingsRoute

    @Serializable
    data object Correction : SettingsRoute

    @Serializable
    data object Theme : SettingsRoute
}
