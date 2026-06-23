package com.mocharealm.kry.android.settings

import android.content.Context

data class KeyboardProfileSpec(
    val id: String,
    val title: String,
    val subtitle: String,
    val shortLabel: String,
    /** Centered on the space bar, like Gboard (e.g. "拼音" / "English"). */
    val spaceLabel: String,
)

object KeyboardProfileStore {
    const val ProfileZhQwerty = "zh-qwerty"
    const val ProfileEnQwerty = "en-qwerty"

    private const val PrefsName = "kry_keyboard_settings"
    private const val KeyActiveProfile = "active_profile"

    val profiles = listOf(
        KeyboardProfileSpec(
            id = ProfileZhQwerty,
            title = "Chinese Pinyin",
            subtitle = "QWERTY tap and glide with Chinese candidates",
            shortLabel = "ZH",
            spaceLabel = "拼音",
        ),
        KeyboardProfileSpec(
            id = ProfileEnQwerty,
            title = "English",
            subtitle = "QWERTY word prediction and glide",
            shortLabel = "EN",
            spaceLabel = "English",
        ),
    )

    fun activeProfile(context: Context): KeyboardProfileSpec {
        val id = preferences(context).getString(KeyActiveProfile, ProfileZhQwerty)
        return profileForId(id)
    }

    fun activeProfileId(context: Context): String {
        return activeProfile(context).id
    }

    fun setActiveProfile(context: Context, profileId: String) {
        preferences(context)
            .edit()
            .putString(KeyActiveProfile, profileForId(profileId).id)
            .apply()
    }

    fun nextProfile(context: Context): KeyboardProfileSpec {
        val current = activeProfile(context)
        val nextIndex = (profiles.indexOfFirst { it.id == current.id } + 1)
            .floorMod(profiles.size)
        return profiles[nextIndex].also { setActiveProfile(context, it.id) }
    }

    fun profileForId(profileId: String?): KeyboardProfileSpec {
        return profiles.firstOrNull { it.id == profileId } ?: profiles.first()
    }

    private fun preferences(context: Context) =
        context.applicationContext.getSharedPreferences(PrefsName, Context.MODE_PRIVATE)

    private fun Int.floorMod(other: Int): Int {
        return ((this % other) + other) % other
    }
}
