package com.mocharealm.kry.android.ui.navigation

import androidx.compose.animation.AnimatedContentTransitionScope
import androidx.compose.animation.ContentTransform
import androidx.compose.animation.EnterExitState
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.scene.Scene
import androidx.navigation3.ui.LocalNavAnimatedContentScope
import androidx.navigation3.ui.defaultTransitionSpec
import androidx.navigationevent.NavigationEvent.Companion.EDGE_LEFT
import androidx.navigationevent.NavigationEventTransitionState
import androidx.navigationevent.NavigationEventTransitionState.InProgress
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

internal interface PredictiveBackAnimationHandler {
    suspend fun onBackPressed(
        transitionState: NavigationEventTransitionState?,
        currentPageKey: NavKey?,
    )

    fun onPagePop(
        contentPageKey: Any,
        animationScope: CoroutineScope,
    )

    @Composable
    fun Modifier.predictiveBackAnimationDecorator(
        transitionState: NavigationEventTransitionState?,
        contentPageKey: Any,
        currentPageKey: NavKey?,
    ): Modifier

    fun onPredictivePopTransitionSpec(
        scope: AnimatedContentTransitionScope<Scene<NavKey>>,
        swipeEdge: Int,
    ): ContentTransform

    fun onPopTransitionSpec(
        scope: AnimatedContentTransitionScope<Scene<NavKey>>,
    ): ContentTransform

    fun onTransitionSpec(
        scope: AnimatedContentTransitionScope<Scene<NavKey>>,
    ): ContentTransform
}

internal class AospPredictiveBackAnimation : PredictiveBackAnimationHandler {
    private var exitingPageKey: String? = null
    private val exitAnimatable = Animatable(0f)
    private var inPredictiveBackAnimation = false
    private var lastSwipeEdge = EDGE_LEFT
    private var lastTouchY: Float? = null
    private var lastGestureProgress = 0f

    override suspend fun onBackPressed(
        transitionState: NavigationEventTransitionState?,
        currentPageKey: NavKey?,
    ) {
        val progressInProgress = transitionState as? InProgress
        if (progressInProgress != null) {
            lastSwipeEdge = progressInProgress.latestEvent.swipeEdge
            lastTouchY = progressInProgress.latestEvent.touchY
            lastGestureProgress = progressInProgress.latestEvent.progress
        }

        val isInterruptingEnter = transitionState is InProgress && !inPredictiveBackAnimation
        if (!isInterruptingEnter) {
            exitingPageKey = currentPageKey.toString()
            exitAnimatable.animateTo(
                targetValue = 1f,
                animationSpec = tween(durationMillis = 150, easing = LinearEasing),
            )
        }
    }

    override fun onPagePop(
        contentPageKey: Any,
        animationScope: CoroutineScope,
    ) {
        if (exitingPageKey == contentPageKey.toString()) {
            exitingPageKey = null
            animationScope.launch {
                exitAnimatable.snapTo(0f)
                lastTouchY = null
                lastGestureProgress = 0f
                lastSwipeEdge = EDGE_LEFT
            }
        }
    }

    @Composable
    override fun Modifier.predictiveBackAnimationDecorator(
        transitionState: NavigationEventTransitionState?,
        contentPageKey: Any,
        currentPageKey: NavKey?,
    ): Modifier = composed {
        val windowInfo = LocalWindowInfo.current
        val navContent = LocalNavAnimatedContentScope.current
        val transition = navContent.transition

        val containerHeightPx = windowInfo.containerSize.height
        val pageKey = contentPageKey.toString()
        val deviceCornerRadius = rememberDeviceCornerRadius()
        val enteringStartOffsetPx = with(LocalDensity.current) { 96.dp.toPx() }

        val linearProgress = exitAnimatable.value
        val emphasizedProgress = CubicBezierEasing(0.2f, 0f, 0f, 1f).transform(linearProgress)

        val progressInProgress = transitionState as? InProgress
        if (progressInProgress != null) {
            lastSwipeEdge = progressInProgress.latestEvent.swipeEdge
            lastTouchY = progressInProgress.latestEvent.touchY
            lastGestureProgress = progressInProgress.latestEvent.progress
        }
        val edge = progressInProgress?.latestEvent?.swipeEdge ?: lastSwipeEdge
        val touchY = progressInProgress?.latestEvent?.touchY ?: lastTouchY
        val gestureProgress = progressInProgress?.latestEvent?.progress ?: lastGestureProgress

        val animatedScale by transition.animateFloat(
            transitionSpec = { tween(300) },
            label = "PredictiveScale",
        ) { state ->
            when (state) {
                EnterExitState.PostExit -> 0.85f
                else -> 1f
            }
        }

        if (pageKey == currentPageKey.toString()) {
            inPredictiveBackAnimation = animatedScale != 1f
        }

        val directionMultiplier = if (edge == EDGE_LEFT) 1f else -1f
        val isExitingPage = exitingPageKey != null && exitingPageKey == pageKey
        val isCurrentNavTarget = exitingPageKey == null && pageKey == currentPageKey.toString()

        val maxScale = 0.85f
        val dragScale = 1f - (1f - maxScale) * gestureProgress
        val currentPivotY = if (touchY != null && containerHeightPx > 0) {
            (touchY / containerHeightPx).coerceIn(0.1f, 0.9f)
        } else {
            0.5f
        }
        val currentPivotX = if (edge == EDGE_LEFT) 0.8f else 0.2f
        val needsClip = transitionState is InProgress && inPredictiveBackAnimation || exitingPageKey != null

        this
            .graphicsLayer {
                if (transitionState is InProgress && !inPredictiveBackAnimation && exitingPageKey == null) {
                    return@graphicsLayer
                }

                if (transitionState is InProgress) {
                    transformOrigin = TransformOrigin(currentPivotX, currentPivotY)
                }

                when {
                    isExitingPage -> {
                        val computedScale = dragScale + (maxScale - dragScale) * emphasizedProgress
                        scaleX = computedScale
                        scaleY = computedScale
                        translationX = enteringStartOffsetPx * directionMultiplier * emphasizedProgress
                        alpha = if (linearProgress >= 0.2f) 0f else (1f - linearProgress * 5f).coerceAtLeast(0f)
                    }

                    isCurrentNavTarget -> {
                        scaleX = dragScale
                        scaleY = dragScale
                        alpha = 1f
                    }

                    else -> {
                        val initialTranslationX = -enteringStartOffsetPx * directionMultiplier
                        if (exitingPageKey != null) {
                            val computedScale = dragScale + (1f - dragScale) * emphasizedProgress
                            scaleX = computedScale
                            scaleY = computedScale
                            translationX = initialTranslationX * (1f - emphasizedProgress)
                        } else if (transitionState is InProgress) {
                            scaleX = dragScale
                            scaleY = dragScale
                            translationX = initialTranslationX
                        }
                        alpha = 1f
                    }
                }
            }
            .clip(if (needsClip) RoundedCornerShape(deviceCornerRadius) else RoundedCornerShape(0.dp))
    }

    override fun onPredictivePopTransitionSpec(
        scope: AnimatedContentTransitionScope<Scene<NavKey>>,
        swipeEdge: Int,
    ): ContentTransform = ContentTransform(
        targetContentEnter = EnterTransition.None,
        initialContentExit = ExitTransition.None,
        sizeTransform = null,
    )

    override fun onPopTransitionSpec(
        scope: AnimatedContentTransitionScope<Scene<NavKey>>,
    ): ContentTransform =
        ContentTransform(
            targetContentEnter = slideInHorizontally(initialOffsetX = { -it / 4 }),
            initialContentExit = scaleOut(targetScale = 0.9f) + fadeOut(),
            sizeTransform = null,
        )

    override fun onTransitionSpec(
        scope: AnimatedContentTransitionScope<Scene<NavKey>>,
    ): ContentTransform =
        defaultTransitionSpec<NavKey>().invoke(scope)
}
