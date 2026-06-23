package com.mocharealm.kry.android.settings

import android.content.Context
import androidx.annotation.StringRes
import com.mocharealm.kry.android.R

data class KeyboardProfileSpec(
    val id: String,
    @StringRes val titleRes: Int,
    @StringRes val subtitleRes: Int,
    @StringRes val shortLabelRes: Int,
    /** Centered on the space bar, like Gboard (e.g. "拼音" / "English"). */
    @StringRes val spaceLabelRes: Int,
)

object KeyboardProfileStore {
    const val ProfileZhQwerty = "zh-qwerty"
    const val ProfileEnQwerty = "en-qwerty"

    private const val PrefsName = "kry_keyboard_settings"
    private const val KeyActiveProfile = "active_profile"

    val profiles = listOf(
        KeyboardProfileSpec(
            id = ProfileZhQwerty,
            titleRes = R.string.profile_zh_qwerty_title,
            subtitleRes = R.string.profile_zh_qwerty_subtitle,
            shortLabelRes = R.string.profile_zh_qwerty_short_label,
            spaceLabelRes = R.string.profile_zh_qwerty_space_label,
        ),
        KeyboardProfileSpec(
            id = ProfileEnQwerty,
            titleRes = R.string.profile_en_qwerty_title,
            subtitleRes = R.string.profile_en_qwerty_subtitle,
            shortLabelRes = R.string.profile_en_qwerty_short_label,
            spaceLabelRes = R.string.profile_en_qwerty_space_label,
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
