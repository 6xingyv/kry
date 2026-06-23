package com.mocharealm.kry.android.ui.navigation

import androidx.compose.runtime.staticCompositionLocalOf
import androidx.navigation3.runtime.NavKey

class Navigator(
    val backStack: MutableList<NavKey>,
) {
    fun push(key: NavKey) {
        if (backStack.lastOrNull() == key) return
        backStack.add(key)
    }

    fun replace(key: NavKey) {
        if (backStack.isEmpty()) {
            backStack.add(key)
        } else {
            backStack[backStack.lastIndex] = key
        }
    }

    fun pop(): Boolean {
        if (backStack.size <= 1) return false
        backStack.removeLastOrNull()
        return true
    }

    fun current(): NavKey? = backStack.lastOrNull()
}

val LocalNavigator = staticCompositionLocalOf<Navigator> {
    error("LocalNavigator not provided")
}
