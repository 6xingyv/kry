package com.mocharealm.kry.android.ui.screen.settings.components

import androidx.annotation.DrawableRes
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.LocalIndication
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.RadioButtonDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.toggleableState
import androidx.compose.ui.state.ToggleableState
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import com.mocharealm.kry.android.R

internal val SettingsCornerRadius = 16.dp
private val SettingsConnectionRadius = 5.dp
private val SettingsGroupGap = 2.dp

private val SettingsTopShape = RoundedCornerShape(
    topStart = SettingsCornerRadius,
    topEnd = SettingsCornerRadius,
    bottomStart = SettingsConnectionRadius,
    bottomEnd = SettingsConnectionRadius,
)
private val SettingsMiddleShape = RoundedCornerShape(SettingsConnectionRadius)
private val SettingsBottomShape = RoundedCornerShape(
    topStart = SettingsConnectionRadius,
    topEnd = SettingsConnectionRadius,
    bottomStart = SettingsCornerRadius,
    bottomEnd = SettingsCornerRadius,
)
private val SettingsSingleShape = RoundedCornerShape(SettingsCornerRadius)

private val LocalSettingsItemShape = staticCompositionLocalOf<Shape> { SettingsSingleShape }

@Composable
internal fun SegmentedColumn(
    modifier: Modifier = Modifier,
    title: String? = null,
    contentPadding: PaddingValues = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
    content: SegmentedColumnScope.() -> Unit,
) {
    val scope = SegmentedColumnScope().apply(content)
    if (scope.items.isEmpty()) return

    Column(modifier = modifier.padding(contentPadding)) {
        if (!title.isNullOrBlank()) {
            Text(
                text = title,
                style = MaterialTheme.typography.titleSmall,
                color = MaterialTheme.colorScheme.primary,
                modifier = Modifier.padding(start = 16.dp, top = 8.dp, bottom = 16.dp),
            )
        }

        Column(verticalArrangement = Arrangement.spacedBy(SettingsGroupGap)) {
            scope.items.forEachIndexed { index, item ->
                val shape = when {
                    scope.items.size == 1 -> SettingsSingleShape
                    index == 0 -> SettingsTopShape
                    index == scope.items.lastIndex -> SettingsBottomShape
                    else -> SettingsMiddleShape
                }
                CompositionLocalProvider(LocalSettingsItemShape provides shape) {
                    item(shape)
                }
            }
        }
    }
}

internal class SegmentedColumnScope {
    internal val items = mutableListOf<@Composable (Shape) -> Unit>()

    fun item(content: @Composable (Shape) -> Unit) {
        items += content
    }
}

@Composable
internal fun BaseSettingItem(
    @DrawableRes icon: Int? = null,
    title: String,
    modifier: Modifier = Modifier,
    description: String? = null,
    enabled: Boolean = true,
    selected: Boolean = false,
    onClick: (() -> Unit)? = null,
    titleStyle: TextStyle = MaterialTheme.typography.titleMedium,
    trailingContent: @Composable ((MutableInteractionSource) -> Unit)? = null,
) {
    val alpha = if (enabled) 1f else 0.38f
    val interactionSource = androidx.compose.runtime.remember { MutableInteractionSource() }
    val shape = LocalSettingsItemShape.current
    val containerColor = if (selected) {
        MaterialTheme.colorScheme.primaryContainer
    } else {
        MaterialTheme.colorScheme.surfaceBright
    }
    val contentColor = if (selected) {
        MaterialTheme.colorScheme.onPrimaryContainer
    } else {
        MaterialTheme.colorScheme.onSurface
    }
    val leadingColor = if (selected) {
        contentColor
    } else {
        MaterialTheme.colorScheme.onSurfaceVariant
    }

    val itemModifier = modifier
        .fillMaxWidth()
        .clip(shape)
        .then(
            if (onClick != null) {
                Modifier.clickable(
                    enabled = enabled,
                    interactionSource = interactionSource,
                    indication = LocalIndication.current,
                    onClick = onClick,
                )
            } else {
                Modifier
            },
        )
        .then(
            if (!enabled) Modifier.semantics { disabled() } else Modifier,
        )

    ListItem(
        headlineContent = {
            Text(
                text = title,
                style = titleStyle,
                modifier = Modifier
                    .alpha(alpha)
                    .padding(top = 4.dp, bottom = if (description == null) 4.dp else 0.dp),
            )
        },
        supportingContent = description?.let {
            {
                Text(
                    text = it,
                    style = MaterialTheme.typography.bodyMedium,
                    color = contentColor.copy(alpha = 0.7f),
                    modifier = Modifier
                        .alpha(alpha)
                        .padding(bottom = 4.dp),
                )
            }
        },
        leadingContent = {
            Box(
                modifier = Modifier
                    .size(24.dp)
                    .alpha(alpha),
                contentAlignment = Alignment.Center,
            ) {
                if (icon != null) {
                    Icon(
                        painter = painterResource(icon),
                        contentDescription = null,
                        tint = leadingColor,
                    )
                } else {
                    Spacer(Modifier.size(24.dp))
                }
            }
        },
        trailingContent = trailingContent?.let {
            {
                Box(
                    modifier = Modifier.alpha(alpha),
                    contentAlignment = Alignment.Center,
                ) {
                    it(interactionSource)
                }
            }
        },
        colors = ListItemDefaults.colors(
            containerColor = containerColor,
            headlineColor = contentColor,
            leadingIconColor = leadingColor,
            supportingColor = contentColor.copy(alpha = 0.7f),
            trailingIconColor = leadingColor,
        ),
        modifier = itemModifier,
    )
}

@Composable
internal fun NavigationSettingItem(
    @DrawableRes icon: Int,
    title: String,
    description: String? = null,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    BaseSettingItem(
        icon = icon,
        title = title,
        description = description,
        enabled = enabled,
        onClick = onClick,
        trailingContent = {
            Icon(
                painter = painterResource(R.drawable.ic_arrow_forward_24px),
                contentDescription = null,
            )
        },
    )
}

@Composable
internal fun RadioSettingItem(
    title: String,
    description: String? = null,
    selected: Boolean,
    onClick: () -> Unit,
) {
    BaseSettingItem(
        title = title,
        description = description,
        selected = selected,
        onClick = onClick,
        modifier = Modifier.semantics(mergeDescendants = true) {
            role = Role.RadioButton
            this.selected = selected
        },
        trailingContent = { interactionSource ->
            CompositionLocalProvider(LocalContentColor provides MaterialTheme.colorScheme.primary) {
                RadioButton(
                    selected = selected,
                    onClick = null,
                    modifier = Modifier.clearAndSetSemantics {},
                    colors = RadioButtonDefaults.colors(selectedColor = LocalContentColor.current),
                    interactionSource = interactionSource,
                )
            }
        },
    )
}

@Composable
internal fun SwitchSettingItem(
    @DrawableRes icon: Int,
    title: String,
    description: String? = null,
    checked: Boolean,
    enabled: Boolean = true,
    onCheckedChange: (Boolean) -> Unit,
) {
    BaseSettingItem(
        icon = icon,
        title = title,
        description = description,
        enabled = enabled,
        onClick = { if (enabled) onCheckedChange(!checked) },
        modifier = Modifier.semantics(mergeDescendants = true) {
            role = Role.Switch
            toggleableState = if (checked) ToggleableState.On else ToggleableState.Off
        },
        trailingContent = { interactionSource ->
            Switch(
                checked = checked,
                onCheckedChange = null,
                enabled = enabled,
                modifier = Modifier.clearAndSetSemantics {},
                interactionSource = interactionSource,
            )
        },
    )
}

@Composable
internal fun SettingsBottomSpacer() {
    Spacer(Modifier.height(24.dp))
}
