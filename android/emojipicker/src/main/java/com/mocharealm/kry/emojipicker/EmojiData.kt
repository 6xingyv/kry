/*
 * Emoji data loaded directly from Unicode's official `emoji-test.txt` (bundled as a raw
 * resource): the authoritative groups (categories), CLDR display order, full emoji list,
 * and skin-tone variants. Renderability-filtered (PaintCompat.hasGlyph) to avoid "tofu".
 *
 * Skin-tone variants (codepoints U+1F3FB..U+1F3FF) are folded under their base emoji and
 * surfaced via the long-press popup; everything else (incl. gendered ZWJ forms, which
 * Unicode lists as distinct emoji) is a grid cell.
 */
package com.mocharealm.kry.emojipicker

import android.content.Context
import android.text.TextPaint
import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.core.graphics.PaintCompat

/** A displayed emoji and its variants (skin tones). Empty [variants] = no popup. */
data class EmojiItem(val emoji: String, val variants: List<String>)

/** A category: header icon + (Unicode group) name + its emoji items. */
class EmojiCategory(
    @DrawableRes val iconRes: Int,
    @StringRes val titleRes: Int,
    val items: List<EmojiItem>,
)

internal object EmojiData {

    @Volatile private var cached: List<EmojiCategory>? = null
    /** variant -> the full skin-tone family it belongs to (drives the long-press popup). */
    @Volatile private var variantsLookup: Map<String, List<String>> = emptyMap()

    private val paint = TextPaint()
    private const val VARIATION_SELECTOR = "️"

    // Unicode group name -> our category icon. Insertion order = display order, which already
    // matches emoji-test.txt (CLDR order). "Component" and any unknown group are skipped.
    private data class CategorySpec(@DrawableRes val iconRes: Int, @StringRes val titleRes: Int)

    private val GROUP_ICONS =
        linkedMapOf(
            "Smileys & Emotion" to CategorySpec(R.drawable.ic_emoji_emotions_24px, R.string.emoji_category_smileys_emotion),
            "People & Body" to CategorySpec(R.drawable.ic_emoji_people_24px, R.string.emoji_category_people_body),
            "Animals & Nature" to CategorySpec(R.drawable.ic_emoji_nature_24px, R.string.emoji_category_animals_nature),
            "Food & Drink" to CategorySpec(R.drawable.ic_emoji_food_24px, R.string.emoji_category_food_drink),
            "Travel & Places" to CategorySpec(R.drawable.ic_emoji_travel_24px, R.string.emoji_category_travel_places),
            "Activities" to CategorySpec(R.drawable.ic_emoji_activity_24px, R.string.emoji_category_activities),
            "Objects" to CategorySpec(R.drawable.ic_emoji_objects_24px, R.string.emoji_category_objects),
            "Symbols" to CategorySpec(R.drawable.ic_emoji_symbols_24px, R.string.emoji_category_symbols),
            "Flags" to CategorySpec(R.drawable.ic_emoji_flags_24px, R.string.emoji_category_flags),
        )

    fun variantsOf(emoji: String): List<String> = variantsLookup[emoji].orEmpty()

    /** Parse + renderability-filter `emoji-test.txt`. Heavy — call off the main thread; cached. */
    fun load(context: Context): List<EmojiCategory> {
        cached?.let { return it }
        return synchronized(this) { cached ?: parse(context).also { cached = it } }
    }

    private fun parse(context: Context): List<EmojiCategory> {
        // group -> (skin-tone-stripped key -> ordered emoji forms [base first, then its skin tones])
        val groups = LinkedHashMap<String, LinkedHashMap<String, MutableList<String>>>()
        var group: String? = null
        context.resources.openRawResource(R.raw.emoji_test).bufferedReader().useLines { lines ->
            for (raw in lines) {
                if (raw.startsWith("# group:")) {
                    group = raw.removePrefix("# group:").trim()
                    continue
                }
                if (raw.isEmpty() || raw.startsWith("#")) continue
                val g = group ?: continue
                if (g !in GROUP_ICONS) continue
                val semicolon = raw.indexOf(';')
                val hash = if (semicolon >= 0) raw.indexOf('#', semicolon) else -1
                if (semicolon < 0 || hash < 0) continue
                if (raw.substring(semicolon + 1, hash).trim() != "fully-qualified") continue
                val codepoints = raw.substring(0, semicolon).trim().split(' ').filter { it.isNotEmpty() }
                val emoji = codepointsToString(codepoints) ?: continue
                val key = codepointsToString(codepoints.filterNot(::isSkinToneHex)) ?: emoji
                groups.getOrPut(g) { LinkedHashMap() }.getOrPut(key) { mutableListOf() }.add(emoji)
            }
        }

        val lookup = HashMap<String, List<String>>()
        val categories =
            GROUP_ICONS.entries.mapNotNull { (groupName, spec) ->
                val keyed = groups[groupName] ?: return@mapNotNull null
                val items =
                    keyed.values.mapNotNull { forms ->
                        val renderable = forms.filter(::isRenderable)
                        if (renderable.isEmpty()) return@mapNotNull null
                        val variants = if (renderable.size > 1) renderable else emptyList()
                        variants.forEach { lookup[it] = variants }
                        EmojiItem(renderable.first(), variants)
                    }
                if (items.isEmpty()) null else EmojiCategory(spec.iconRes, spec.titleRes, items)
            }
        variantsLookup = lookup
        return categories
    }

    private fun codepointsToString(hex: List<String>): String? {
        if (hex.isEmpty()) return null
        val sb = StringBuilder()
        for (h in hex) sb.appendCodePoint(h.toIntOrNull(16) ?: return null)
        return sb.toString()
    }

    private fun isSkinToneHex(hex: String): Boolean =
        (hex.toIntOrNull(16) ?: return false) in 0x1F3FB..0x1F3FF

    /** Keep only emojis the device can render; retry without the variation selector for old glyphs. */
    private fun isRenderable(emoji: String): Boolean =
        hasGlyph(emoji) || hasGlyph(emoji.replace(VARIATION_SELECTOR, ""))

    private fun hasGlyph(s: String): Boolean = s.isNotEmpty() && PaintCompat.hasGlyph(paint, s)
}
