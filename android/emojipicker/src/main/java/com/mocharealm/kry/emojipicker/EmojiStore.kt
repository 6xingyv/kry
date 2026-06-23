/*
 * Compose port of DefaultRecentEmojiProvider + StickyVariantProvider — recently used
 * emojis and the user's per-emoji skin-tone choice, persisted in SharedPreferences.
 */
package com.mocharealm.kry.emojipicker

import android.content.Context

internal class EmojiStore(context: Context) {
    private val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    private val recent: MutableList<String> =
        prefs.getString(KEY_RECENT, null)
            ?.split(SEP)
            ?.filter { it.isNotEmpty() }
            ?.toMutableList() ?: mutableListOf()

    private val sticky: MutableMap<String, String> =
        prefs.getString(KEY_STICKY, null)
            ?.split(ENTRY_SEP)
            ?.mapNotNull {
                it.split(KV_SEP, limit = 2).takeIf { kv -> kv.size == 2 }?.let { kv -> kv[0] to kv[1] }
            }
            ?.toMap()
            ?.toMutableMap() ?: mutableMapOf()

    fun recentList(): List<String> = recent.toList()

    fun recordSelection(emoji: String) {
        recent.remove(emoji)
        recent.add(0, emoji)
        prefs.edit().putString(KEY_RECENT, recent.joinToString(SEP)).apply()
    }

    fun stickyMap(): Map<String, String> = sticky.toMap()

    /** Remember (or clear, when the base is re-selected) the chosen variant for a base emoji. */
    fun updateSticky(baseEmoji: String, chosen: String) {
        if (baseEmoji == chosen) sticky.remove(baseEmoji) else sticky[baseEmoji] = chosen
        prefs
            .edit()
            .putString(KEY_STICKY, sticky.entries.joinToString(ENTRY_SEP) { "${it.key}$KV_SEP${it.value}" })
            .apply()
    }

    companion object {
        private const val PREFS = "com.mocharealm.kry.emojipicker"
        private const val KEY_RECENT = "recent_emoji"
        private const val KEY_STICKY = "sticky_variant"
        private const val SEP = ","
        private const val ENTRY_SEP = "|"
        private const val KV_SEP = "="
    }
}
