/*
 * Compose + Material 3 port of androidx.emoji2.emojipicker.EmojiPickerView.
 *
 * A vertically scrollable emoji grid with a clickable category tab header (synced to
 * scroll), a "recently used" section, per-emoji sticky skin-tone selection, and a
 * long-press variant popup. Renderability-filtered bundled emoji data (see EmojiData).
 */
package com.mocharealm.kry.emojipicker

import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.PrimaryScrollableTabRow
import androidx.compose.material3.Surface
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import kotlin.math.roundToInt
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The emoji picker panel.
 *
 * @param onEmojiPicked invoked with the chosen emoji string (already resolved to the chosen variant)
 * @param onBackspace invoked when the backspace key is tapped
 * @param onClose invoked when the user wants to leave the panel (the "ABC" key)
 * @param columns number of emoji columns (default 8)
 */
@OptIn(ExperimentalFoundationApi::class, ExperimentalMaterial3Api::class)
@Composable
fun EmojiPicker(
    onEmojiPicked: (String) -> Unit,
    onBackspace: () -> Unit,
    onClose: () -> Unit,
    modifier: Modifier = Modifier,
    columns: Int = 8,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val store = remember { EmojiStore(context) }

    var categories by remember { mutableStateOf<List<EmojiCategory>>(emptyList()) }
    var recent by remember { mutableStateOf<List<String>>(emptyList()) }
    val sticky = remember { mutableStateMapOf<String, String>().apply { putAll(store.stickyMap()) } }
    var variantRequest by remember { mutableStateOf<VariantRequest?>(null) }

    LaunchedEffect(Unit) {
        recent = store.recentList()
        categories = withContext(Dispatchers.Default) { EmojiData.load(context) }
    }

    Surface(modifier = modifier.fillMaxSize(), color = MaterialTheme.colorScheme.surfaceContainer) {
        if (categories.isEmpty()) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                CircularProgressIndicator()
            }
            return@Surface
        }

        // Sections: a "recently used" group followed by the bundled categories.
        val sections =
            remember(categories, recent) {
                buildList {
                    add(
                        EmojiSection(
                            R.drawable.ic_emoji_recent_24px,
                            R.string.emoji_category_recent,
                            recent.map { EmojiItem(it, EmojiData.variantsOf(it)) },
                            isRecent = true,
                        )
                    )
                    categories.forEach { add(EmojiSection(it.iconRes, it.titleRes, it.items, isRecent = false)) }
                }
            }
        // The flat LazyGrid item index of each section's header (header spans a full line = 1 item;
        // each emoji cell = 1 item; an empty recent section contributes a 1-item placeholder).
        val headerIndices =
            remember(sections) {
                var idx = 0
                sections.map { s ->
                    val at = idx
                    idx += 1 + if (s.items.isEmpty()) 1 else s.items.size
                    at
                }
            }
        val gridState = rememberLazyGridState()
        val selectedTab by
            remember(headerIndices) {
                derivedStateOf {
                    headerIndices.indexOfLast { it <= gridState.firstVisibleItemIndex }.coerceAtLeast(0)
                }
            }

        Column(Modifier.fillMaxSize()) {
            PrimaryScrollableTabRow(
                selectedTabIndex = selectedTab,
                containerColor = MaterialTheme.colorScheme.surfaceContainer,
                edgePadding = 0.dp,
                modifier = Modifier.fillMaxWidth(),
            ) {
                sections.forEachIndexed { i, s ->
                    val title = stringResource(s.titleRes)
                    Tab(
                        selected = i == selectedTab,
                        onClick = { scope.launch { gridState.animateScrollToItem(headerIndices[i]) } },
                        icon = {
                            Icon(
                                painterResource(s.iconRes),
                                contentDescription = title,
                                modifier = Modifier.size(20.dp),
                            )
                        },
                    )
                }
            }

            LazyVerticalGrid(
                columns = GridCells.Fixed(columns),
                state = gridState,
                modifier = Modifier.weight(1f).fillMaxWidth(),
            ) {
                sections.forEach { section ->
                    item(span = { GridItemSpan(maxLineSpan) }) { SectionHeader(stringResource(section.titleRes)) }
                    if (section.items.isEmpty()) {
                        item(span = { GridItemSpan(maxLineSpan) }) {
                            Text(
                                stringResource(R.string.emoji_no_recent),
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                fontSize = 13.sp,
                                modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                            )
                        }
                    } else {
                        items(section.items, key = { "${section.titleRes}-${it.emoji}" }) { item ->
                            val display = sticky[item.emoji] ?: item.emoji
                            EmojiCell(
                                emoji = display,
                                onPick = {
                                    onEmojiPicked(display)
                                    store.recordSelection(display)
                                },
                                onLongPress = { bounds ->
                                    if (item.variants.size > 1) variantRequest = VariantRequest(item, bounds)
                                },
                            )
                        }
                    }
                }
            }

            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
            Row(
                modifier = Modifier.fillMaxWidth().height(48.dp).padding(horizontal = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                TextButton(onClick = onClose) {
                    Text(stringResource(R.string.emoji_close_abc), fontWeight = FontWeight.Medium, fontSize = 15.sp)
                }
                Spacer(Modifier.weight(1f))
                IconButton(onClick = onBackspace) {
                    Icon(
                        painterResource(R.drawable.ic_emoji_backspace_24px),
                        contentDescription = stringResource(R.string.emoji_backspace),
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }

    variantRequest?.let { req ->
        VariantPopup(
            variants = req.item.variants,
            anchor = req.bounds,
            onPick = { chosen ->
                onEmojiPicked(chosen)
                store.recordSelection(chosen)
                store.updateSticky(req.item.emoji, chosen)
                if (req.item.emoji == chosen) sticky.remove(req.item.emoji) else sticky[req.item.emoji] = chosen
                variantRequest = null
            },
            onDismiss = { variantRequest = null },
        )
    }
}

private class EmojiSection(
    @DrawableRes val iconRes: Int,
    @StringRes val titleRes: Int,
    val items: List<EmojiItem>,
    val isRecent: Boolean,
)

private class VariantRequest(val item: EmojiItem, val bounds: Rect)

@Composable
private fun SectionHeader(title: String) {
    Text(
        text = title,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        fontSize = 12.sp,
        fontWeight = FontWeight.Medium,
        modifier = Modifier.fillMaxWidth().padding(start = 16.dp, top = 10.dp, bottom = 4.dp),
    )
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun EmojiCell(emoji: String, onPick: () -> Unit, onLongPress: (Rect) -> Unit) {
    var bounds by remember { mutableStateOf(Rect.Zero) }
    Box(
        modifier =
            Modifier.aspectRatio(1f)
                .onGloballyPositioned { bounds = it.boundsInWindow() }
                .combinedClickable(onClick = onPick, onLongClick = { onLongPress(bounds) }),
        contentAlignment = Alignment.Center,
    ) {
        Text(emoji, fontSize = 22.sp, textAlign = TextAlign.Center)
    }
}

@Composable
private fun VariantPopup(
    variants: List<String>,
    anchor: Rect,
    onPick: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val columns = minOf(6, variants.size).coerceAtLeast(1)
    Popup(
        popupPositionProvider = remember(anchor) { AnchorAbovePositionProvider(anchor) },
        onDismissRequest = onDismiss,
    ) {
        Surface(
            shape = RoundedCornerShape(12.dp),
            color = MaterialTheme.colorScheme.surfaceContainerHighest,
            tonalElevation = 3.dp,
            shadowElevation = 8.dp,
        ) {
            Column(Modifier.padding(4.dp), verticalArrangement = Arrangement.spacedBy(0.dp)) {
                variants.chunked(columns).forEach { rowVariants ->
                    Row {
                        rowVariants.forEach { v ->
                            Box(
                                modifier =
                                    Modifier.size(44.dp)
                                        .clip(RoundedCornerShape(8.dp))
                                        .clickable { onPick(v) },
                                contentAlignment = Alignment.Center,
                            ) {
                                Text(v, fontSize = 24.sp, textAlign = TextAlign.Center)
                            }
                        }
                    }
                }
            }
        }
    }
}

/** Positions a popup centered horizontally above the anchor rect (clamped to the window). */
private class AnchorAbovePositionProvider(private val anchor: Rect) : PopupPositionProvider {
    override fun calculatePosition(
        anchorBounds: IntRect,
        windowSize: IntSize,
        layoutDirection: LayoutDirection,
        popupContentSize: IntSize,
    ): IntOffset {
        val cx = anchor.left.roundToInt() + (anchor.width.roundToInt() - popupContentSize.width) / 2
        val x = cx.coerceIn(0, (windowSize.width - popupContentSize.width).coerceAtLeast(0))
        val y = (anchor.top.roundToInt() - popupContentSize.height).coerceAtLeast(0)
        return IntOffset(x, y)
    }
}
