package com.mocharealm.kry.android.ui.ime

import android.view.inputmethod.EditorInfo
import androidx.annotation.DrawableRes
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.mocharealm.kry.android.R
import com.mocharealm.kry.android.ime.EnterKeySpec
import com.mocharealm.kry.emojipicker.EmojiPicker
import com.mocharealm.kry.android.nativebridge.Candidate
import com.mocharealm.kry.android.settings.KeyboardProfileSpec
import com.mocharealm.kry.android.ui.theme.KryTheme

private const val Row0 = "qwertyuiop"
private const val Row0Digits = "1234567890"
private const val Row1 = "asdfghjkl"
private const val Row2 = "zxcvbnm"

// Symbols (?123) layer — direct-insert keys, not decoded by the capture overlay.
private val SymRow0 = listOf("1", "2", "3", "4", "5", "6", "7", "8", "9", "0")
private val SymRow1 = listOf("@", "#", "$", "%", "&", "-", "+", "(", ")", "/")
private val SymRow2 = listOf("*", "\"", "'", ":", ";", "!", "?")

private object KeyboardDimens {
    val topBandHeight = 56.dp
    val keyGridHeight = 220.dp
    val bottomRowHeight = 64.dp
    val surfacePadding = 8.dp
    val toolbarHorizontalPadding = 12.dp
    val toolButtonSize = 40.dp
    val keyHorizontalPadding = 3.dp
    val keyVerticalPadding = 4.dp
    val bottomKeyGap = 6.dp
}

/** Shift latch state: lowercase, one-shot capital, or locked caps. */
enum class ShiftState { Off, Shifted, Locked }

// Letter-row vertical layout MUST mirror Phone10ColGeometry (key_h=0.25, row_gap=0.04,
// total y-extent 0.83) so a tap aligns with the geometry key the engine decodes against.
// Using these as Column weights places row centers at 0.151 / 0.5 / 0.849 of the grid
// (= geometry y 0.125 / 0.415 / 0.705 over 0.83) instead of even thirds.
private const val GeoKeyH = 0.25f
private const val GeoRowGap = 0.04f

@Composable
fun KryKeyboardSurface(
    candidates: List<Candidate>,
    activeProfile: KeyboardProfileSpec,
    enterKey: EnterKeySpec,
    canSwitchIme: Boolean,
    onCandidate: (Candidate) -> Unit,
    onSpace: () -> Unit,
    onBackspace: () -> Unit,
    onEnter: () -> Unit,
    onSwitchProfile: () -> Unit,
    onOpenSettings: () -> Unit,
    onSwitchIme: () -> Unit,
    // Shift latch, symbols (?123) layer, emoji, and punctuation. Defaults keep
    // callers valid; the host owns the state and supplies the handlers.
    shiftState: ShiftState = ShiftState.Off,
    symbolsMode: Boolean = false,
    emojiMode: Boolean = false,
    chinesePunct: Boolean = false,
    onShift: () -> Unit = {},
    onSymbols: () -> Unit = {},
    onEmoji: () -> Unit = {},
    onEmojiPicked: (String) -> Unit = {},
    onCloseEmoji: () -> Unit = {},
    onPunct: (String) -> Unit = {},
    onSymbolInput: (String) -> Unit = {},
) {
    KryTheme {
        Surface(
            modifier = Modifier.fillMaxSize(),
            color = MaterialTheme.colorScheme.surfaceContainer,
            tonalElevation = 3.dp,
        ) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .navigationBarsPadding(),
            ) {
                if (emojiMode) {
                    EmojiPicker(
                        onEmojiPicked = onEmojiPicked,
                        onBackspace = onBackspace,
                        onClose = onCloseEmoji,
                        modifier = Modifier.fillMaxSize(),
                    )
                    return@Column
                }

                // Top band (56dp): toolbar when idle, candidate strip while typing.
                // Height is fixed so the capture overlay's topMargin stays aligned.
                if (candidates.isEmpty()) {
                    Toolbar(
                        onOpenSettings = onOpenSettings,
                        modifier = Modifier.fillMaxWidth().height(KeyboardDimens.topBandHeight),
                    )
                } else {
                    CandidateStrip(
                        candidates = candidates,
                        onCandidate = onCandidate,
                        modifier = Modifier.fillMaxWidth().height(KeyboardDimens.topBandHeight),
                    )
                }
                KeyboardRows(
                    shiftState = shiftState,
                    symbolsMode = symbolsMode,
                    onShift = onShift,
                    onBackspace = onBackspace,
                    onSymbolInput = onSymbolInput,
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(KeyboardDimens.keyGridHeight)
                        .padding(KeyboardDimens.surfacePadding),
                )
                BottomRow(
                    spaceLabel = stringResource(activeProfile.spaceLabelRes),
                    symbolsMode = symbolsMode,
                    chinesePunct = chinesePunct,
                    enterIcon = when (enterKey.actionId) {
                        EditorInfo.IME_ACTION_SEARCH -> R.drawable.ic_search_24px
                        EditorInfo.IME_ACTION_SEND -> R.drawable.ic_send_24px
                        else -> R.drawable.ic_keyboard_return_24px
                    },
                    onSymbols = onSymbols,
                    onEmoji = onEmoji,
                    onComma = { onPunct(if (chinesePunct) "，" else ",") },
                    onPeriod = { onPunct(if (chinesePunct) "。" else ".") },
                    onSpace = onSpace,
                    onSwitchProfile = onSwitchProfile,
                    onEnter = onEnter,
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(KeyboardDimens.bottomRowHeight)
                        .padding(
                            start = KeyboardDimens.surfacePadding,
                            end = KeyboardDimens.surfacePadding,
                            bottom = KeyboardDimens.surfacePadding,
                        ),
                )
            }
        }
    }
}

// ── Top toolbar ────────────────────────────────────────────────────────────────

@Composable
private fun Toolbar(onOpenSettings: () -> Unit, modifier: Modifier = Modifier) {
    Row(
        modifier = modifier.padding(horizontal = KeyboardDimens.toolbarHorizontalPadding),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        ToolIcon(R.drawable.ic_grid_view_24px, onClick = onOpenSettings)
        ToolIcon(R.drawable.ic_sticker_24px)
        ToolIcon(R.drawable.ic_gif_24px)
        ToolIcon(R.drawable.ic_clipboard_24px)
        ToolIcon(R.drawable.ic_settings_24px, onClick = onOpenSettings)
        ToolIcon(R.drawable.ic_text_edit_24px)
        ToolIcon(R.drawable.ic_mic_24px, filled = true)
    }
}

@Composable
private fun ToolIcon(
    @DrawableRes icon: Int,
    filled: Boolean = false,
    onClick: () -> Unit = {},
) {
    Box(
        modifier = Modifier
            .size(KeyboardDimens.toolButtonSize)
            .clip(CircleShape)
            .then(
                if (filled) Modifier.background(MaterialTheme.colorScheme.surfaceVariant)
                else Modifier
            )
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            painter = painterResource(icon),
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.size(24.dp),
        )
    }
}

// ── Candidate strip (shown while typing) ────────────────────────────────────────

@Composable
private fun CandidateStrip(
    candidates: List<Candidate>,
    onCandidate: (Candidate) -> Unit,
    modifier: Modifier = Modifier,
) {
    // Gboard-style suggestion strip: scrollable plain-text candidates separated by thin
    // vertical dividers (no chips). The first/best one is emphasized.
    Row(
        modifier = modifier.horizontalScroll(rememberScrollState()),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        for ((index, candidate) in candidates.withIndex()) {
            if (index > 0) {
                Box(
                    Modifier
                        .height(22.dp)
                        .width(1.dp)
                        .background(MaterialTheme.colorScheme.outlineVariant),
                )
            }
            Box(
                modifier = Modifier
                    .fillMaxHeight()
                    .clickable { onCandidate(candidate) }
                    .padding(horizontal = 18.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    candidate.text,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    fontSize = 20.sp,
                    fontWeight = if (index == 0) FontWeight.Medium else FontWeight.Normal,
                    color = MaterialTheme.colorScheme.onSurface,
                )
            }
        }
    }
}

// ── Letter rows (the geometry-aligned capture grid) ─────────────────────────────

@Composable
private fun KeyboardRows(
    shiftState: ShiftState,
    symbolsMode: Boolean,
    onShift: () -> Unit,
    onBackspace: () -> Unit,
    onSymbolInput: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    if (symbolsMode) {
        SymbolRows(onBackspace = onBackspace, onSymbolInput = onSymbolInput, modifier = modifier)
        return
    }
    val upper = shiftState != ShiftState.Off
    Column(modifier = modifier) {
        LetterRow(weight = GeoKeyH) {
            Row0.forEachIndexed { i, ch ->
                LetterKey(ch, upper, topLabel = Row0Digits[i].toString(), modifier = Modifier.weight(1f))
            }
        }
        Spacer(Modifier.weight(GeoRowGap))
        LetterRow(weight = GeoKeyH) {
            Spacer(Modifier.weight(0.5f)) // geometry row offset 0.05 = half a key
            Row1.forEach { LetterKey(it, upper, modifier = Modifier.weight(1f)) }
            Spacer(Modifier.weight(0.5f))
        }
        Spacer(Modifier.weight(GeoRowGap))
        LetterRow(weight = GeoKeyH) {
            // Shift/Backspace occupy the geometry's 1.5-key row-2 indents. Touches on
            // them are routed to these keys via MotionCaptureView.passthroughZones; the
            // letters stay under the capture overlay for tap/glide decoding.
            ShiftKey(shiftState, onClick = onShift, modifier = Modifier.weight(1.5f))
            Row2.forEach { LetterKey(it, upper, modifier = Modifier.weight(1f)) }
            FuncKey(R.drawable.ic_backspace_24px, onClick = onBackspace, tinted = true, modifier = Modifier.weight(1.5f))
        }
    }
}

// ── Symbols (?123) layer — direct-insert keys ───────────────────────────────────

@Composable
private fun SymbolRows(
    onBackspace: () -> Unit,
    onSymbolInput: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier) {
        LetterRow(weight = GeoKeyH) {
            SymRow0.forEach { SymKey(it, onSymbolInput, Modifier.weight(1f)) }
        }
        Spacer(Modifier.weight(GeoRowGap))
        LetterRow(weight = GeoKeyH) {
            SymRow1.forEach { SymKey(it, onSymbolInput, Modifier.weight(1f)) }
        }
        Spacer(Modifier.weight(GeoRowGap))
        LetterRow(weight = GeoKeyH) {
            Spacer(Modifier.weight(1.5f))
            SymRow2.forEach { SymKey(it, onSymbolInput, Modifier.weight(1f)) }
            FuncKey(R.drawable.ic_backspace_24px, onClick = onBackspace, tinted = true, modifier = Modifier.weight(1.5f))
        }
    }
}

@Composable
private fun RowScope.SymKey(sym: String, onInput: (String) -> Unit, modifier: Modifier = Modifier) {
    Box(
        modifier = modifier
            .fillMaxHeight()
            .padding(
                horizontal = KeyboardDimens.keyHorizontalPadding,
                vertical = KeyboardDimens.keyVerticalPadding,
            )
            .clip(MaterialTheme.shapes.small)
            .background(MaterialTheme.colorScheme.surfaceContainerHighest)
            .clickable { onInput(sym) },
        contentAlignment = Alignment.Center,
    ) {
        Text(sym, color = MaterialTheme.colorScheme.onSurface, fontSize = 20.sp, fontWeight = FontWeight.Medium)
    }
}

@Composable
private fun RowScope.ShiftKey(shiftState: ShiftState, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val background = when (shiftState) {
        ShiftState.Off -> MaterialTheme.colorScheme.surfaceContainerHigh
        ShiftState.Shifted -> MaterialTheme.colorScheme.primaryContainer
        ShiftState.Locked -> MaterialTheme.colorScheme.primary
    }
    val tint = when (shiftState) {
        ShiftState.Off -> MaterialTheme.colorScheme.onSecondaryContainer
        ShiftState.Shifted -> MaterialTheme.colorScheme.onPrimaryContainer
        ShiftState.Locked -> MaterialTheme.colorScheme.onPrimary
    }
    Box(
        modifier = modifier
            .fillMaxHeight()
            .padding(
                horizontal = KeyboardDimens.keyHorizontalPadding,
                vertical = KeyboardDimens.keyVerticalPadding,
            )
            .clip(MaterialTheme.shapes.small)
            .background(background)
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            painterResource(R.drawable.ic_shift_24px),
            contentDescription = stringResource(R.string.content_description_shift),
            tint = tint,
            modifier = Modifier.size(24.dp),
        )
    }
}

@Composable
private fun ColumnScope.LetterRow(weight: Float, content: @Composable RowScope.() -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth().weight(weight),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
        content = content,
    )
}

@Composable
private fun RowScope.LetterKey(
    ch: Char,
    upper: Boolean = false,
    topLabel: String? = null,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier = modifier
            .fillMaxHeight()
            .padding(
                horizontal = KeyboardDimens.keyHorizontalPadding,
                vertical = KeyboardDimens.keyVerticalPadding,
            )
            .clip(MaterialTheme.shapes.small)
            .background(MaterialTheme.colorScheme.surfaceContainerHighest),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = if (upper) ch.uppercase() else ch.lowercase(),
            color = MaterialTheme.colorScheme.onSurface,
            fontSize = 20.sp,
            fontWeight = FontWeight.Medium,
        )
        if (topLabel != null) {
            Text(
                text = topLabel,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.55f),
                fontSize = 10.sp,
                modifier = Modifier.align(Alignment.TopEnd).padding(top = 3.dp, end = 6.dp),
            )
        }
    }
}

@Composable
private fun RowScope.FuncKey(
    @DrawableRes icon: Int,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    tinted: Boolean = false,
) {
    Box(
        modifier = modifier
            .fillMaxHeight()
            .padding(
                horizontal = KeyboardDimens.keyHorizontalPadding,
                vertical = KeyboardDimens.keyVerticalPadding,
            )
            .clip(MaterialTheme.shapes.small)
            .background(
                if (tinted) MaterialTheme.colorScheme.surfaceContainerHigh
                else MaterialTheme.colorScheme.surfaceContainerHighest
            )
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(painterResource(icon), contentDescription = null, modifier = Modifier.size(24.dp))
    }
}

// ── Bottom action row ───────────────────────────────────────────────────────────

@Composable
private fun BottomRow(
    spaceLabel: String,
    symbolsMode: Boolean,
    chinesePunct: Boolean,
    @DrawableRes enterIcon: Int,
    onSymbols: () -> Unit,
    onEmoji: () -> Unit,
    onComma: () -> Unit,
    onPeriod: () -> Unit,
    onSpace: () -> Unit,
    onSwitchProfile: () -> Unit,
    onEnter: () -> Unit,
    modifier: Modifier = Modifier,
) {
    // Gboard bottom row: ?123 | emoji+comma | wide space (long-press = switch language) |
    // period | accent enter circle. No globe key — language switch is long-press on space.
    Row(
        modifier = modifier,
        horizontalArrangement = Arrangement.spacedBy(KeyboardDimens.bottomKeyGap),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TextKey(
            if (symbolsMode) stringResource(R.string.keyboard_symbols_abc) else stringResource(R.string.keyboard_symbols_toggle),
            onClick = onSymbols,
            tinted = true,
            modifier = Modifier.weight(1.5f),
        )
        EmojiCommaKey(
            commaLabel = if (chinesePunct) "，" else ",",
            onClick = onComma,
            onLongClick = onEmoji,
            modifier = Modifier.weight(1f),
        )
        SpaceKey(label = spaceLabel, onTap = onSpace, onLongPress = onSwitchProfile, modifier = Modifier.weight(4.5f))
        TextKey(if (chinesePunct) "。" else ".", onClick = onPeriod, tinted = true, modifier = Modifier.weight(1f))
        EnterKey(icon = enterIcon, onClick = onEnter, modifier = Modifier.weight(1.5f))
    }
}

@Composable
private fun RowScope.TextKey(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    tinted: Boolean = false,
) {
    Box(
        modifier = modifier
            .fillMaxHeight()
            .clip(MaterialTheme.shapes.medium)
            .background(
                if (tinted) MaterialTheme.colorScheme.surfaceContainerHigh
                else MaterialTheme.colorScheme.surfaceContainerHighest
            )
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, fontSize = 15.sp, fontWeight = FontWeight.Medium)
    }
}

/** Gboard's key left of the spacebar: emoji glyph with a small comma label. Tap inserts
 *  the comma (a dedicated emoji panel is a separate feature). */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun RowScope.EmojiCommaKey(
    commaLabel: String,
    onClick: () -> Unit,
    onLongClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier = modifier
            .fillMaxHeight()
            .clip(MaterialTheme.shapes.medium)
            .background(MaterialTheme.colorScheme.surfaceContainerHigh)
            .combinedClickable(onClick = onClick, onLongClick = onLongClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            painterResource(R.drawable.ic_mood_24px),
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.size(22.dp),
        )
        Text(
            commaLabel,
            fontSize = 10.sp,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.align(Alignment.BottomStart).padding(start = 7.dp, bottom = 5.dp),
        )
    }
}

/** Wide space bar. Tap = space; long-press = switch language, like Gboard. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun RowScope.SpaceKey(
    label: String,
    onTap: () -> Unit,
    onLongPress: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier = modifier
            .fillMaxHeight()
            .clip(CircleShape)
            .background(MaterialTheme.colorScheme.surfaceContainerHighest)
            .combinedClickable(onClick = onTap, onLongClick = onLongPress),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, fontSize = 14.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** Accent enter key (filled circle), like Gboard's action key. */
@Composable
private fun RowScope.EnterKey(@DrawableRes icon: Int, onClick: () -> Unit, modifier: Modifier = Modifier) {
    Box(
        modifier = modifier
            .fillMaxHeight()
            .clip(CircleShape)
            .background(MaterialTheme.colorScheme.primaryContainer)
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            painterResource(icon),
            contentDescription = stringResource(R.string.content_description_enter),
            tint = MaterialTheme.colorScheme.onPrimaryContainer,
            modifier = Modifier.size(24.dp),
        )
    }
}
