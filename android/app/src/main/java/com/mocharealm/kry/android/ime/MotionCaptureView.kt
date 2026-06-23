package com.mocharealm.kry.android.ime

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.max

private const val MaxSamples = 96
private const val MotionSampleBytes = 24
private const val TrailHoldMillis = 180L
private const val TapDurationMillis = 240L

class MotionCaptureView(context: Context) : View(context) {
    var onGesture: ((ByteBuffer, Int, Float, Float) -> Unit)? = null
    var onTap: ((Float, Float, Float, Float) -> Unit)? = null

    /**
     * Padding insets (in pixels) that define the offset from this view's edges
     * to the actual key grid area. Touch coordinates are adjusted by these
     * insets so that (0,0) maps to the top-left corner of the key grid,
     * and the effective width/height reflect the key grid dimensions.
     */
    var gridInsetLeft: Float = 0f
    var gridInsetTop: Float = 0f
    var gridInsetRight: Float = 0f
    var gridInsetBottom: Float = 0f

    /**
     * Regions, in GRID-fraction coordinates ([0,1] over the key grid), where touches
     * should NOT be decoded but passed through to the Compose keys underneath (e.g.
     * Shift and Backspace, which sit in the row-2 indents). On ACTION_DOWN inside one
     * of these, [onTouchEvent] returns false so the FrameLayout re-dispatches the
     * event to the composeView below.
     */
    var passthroughGridZones: List<android.graphics.RectF> = emptyList()

    private fun isPassthrough(x: Float, y: Float): Boolean {
        if (passthroughGridZones.isEmpty()) return false
        val fx = (x - gridInsetLeft) / gridWidth
        val fy = (y - gridInsetTop) / gridHeight
        return passthroughGridZones.any { it.contains(fx, fy) }
    }

    private val buffer = ByteBuffer
        .allocateDirect(MaxSamples * MotionSampleBytes)
        .order(ByteOrder.nativeOrder())
    private var sampleCount = 0
    private val trailPath = Path()
    private val trailPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = 0xCC0087FD.toInt()
        style = Paint.Style.STROKE
        strokeWidth = 7f * resources.displayMetrics.density
        strokeCap = Paint.Cap.ROUND
        strokeJoin = Paint.Join.ROUND
    }
    private val tapSlop = ViewConfiguration.get(context).scaledTouchSlop.toFloat()
    private var downX = 0f
    private var downY = 0f
    private var downTime = 0L
    private var maxMoveDistance = 0f
    private var drawingGesture = false
    private var activePointerId = MotionEvent.INVALID_POINTER_ID

    private val clearTrail = Runnable {
        trailPath.reset()
        drawingGesture = false
        invalidate()
    }

    init {
        isClickable = true
        setWillNotDraw(false)
    }

    private val gridWidth: Float
        get() = (width - gridInsetLeft - gridInsetRight).coerceAtLeast(1f)

    private val gridHeight: Float
        get() = (height - gridInsetTop - gridInsetBottom).coerceAtLeast(1f)

    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (event.actionMasked == MotionEvent.ACTION_DOWN && isPassthrough(event.x, event.y)) {
            return false // let the Compose Shift/Backspace key underneath handle it
        }
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                removeCallbacks(clearTrail)
                reset()
                activePointerId = event.getPointerId(0)
                downX = event.x
                downY = event.y
                downTime = event.eventTime
                maxMoveDistance = 0f
                drawingGesture = false
                trailPath.reset()
                trailPath.moveTo(event.x, event.y)
                appendAdjusted(event.x, event.y, event.actionMasked, activePointerId, event.eventTime)
                parent.requestDisallowInterceptTouchEvent(true)
            }

            MotionEvent.ACTION_MOVE -> {
                val pointerIndex = event.findPointerIndex(activePointerId)
                    .takeIf { it >= 0 }
                    ?: 0
                val pointerId = event.getPointerId(pointerIndex)
                for (historyIndex in 0 until event.historySize) {
                    val historicalX = event.getHistoricalX(pointerIndex, historyIndex)
                    val historicalY = event.getHistoricalY(pointerIndex, historyIndex)
                    appendAdjusted(
                        x = historicalX,
                        y = historicalY,
                        action = MotionEvent.ACTION_MOVE,
                        pointerId = pointerId,
                        eventTime = event.getHistoricalEventTime(historyIndex),
                    )
                    appendTrailPoint(historicalX, historicalY)
                }
                appendAdjusted(event.getX(pointerIndex), event.getY(pointerIndex), event.actionMasked, pointerId, event.eventTime)
                appendTrailPoint(event.getX(pointerIndex), event.getY(pointerIndex))
            }

            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                val pointerIndex = event.findPointerIndex(activePointerId)
                    .takeIf { it >= 0 }
                    ?: 0
                val pointerId = event.getPointerId(pointerIndex)
                val x = event.getX(pointerIndex)
                val y = event.getY(pointerIndex)
                appendAdjusted(x, y, event.actionMasked, pointerId, event.eventTime)
                appendTrailPoint(x, y)
                buffer.position(0)
                val isTap = event.actionMasked == MotionEvent.ACTION_UP &&
                    maxMoveDistance <= tapSlop * 1.6f &&
                    event.eventTime - downTime <= TapDurationMillis
                if (isTap) {
                    performClick()
                    onTap?.invoke(
                        x - gridInsetLeft,
                        y - gridInsetTop,
                        gridWidth,
                        gridHeight,
                    )
                    trailPath.reset()
                    drawingGesture = false
                    invalidate()
                } else if (event.actionMasked == MotionEvent.ACTION_UP && sampleCount >= 2) {
                    onGesture?.invoke(buffer, sampleCount, gridWidth, gridHeight)
                    postDelayed(clearTrail, TrailHoldMillis)
                } else {
                    post(clearTrail)
                }
                activePointerId = MotionEvent.INVALID_POINTER_ID
                parent.requestDisallowInterceptTouchEvent(false)
            }
        }
        return true
    }

    override fun performClick(): Boolean {
        super.performClick()
        return true
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        if (drawingGesture && !trailPath.isEmpty) {
            canvas.drawPath(trailPath, trailPaint)
        }
    }

    private fun reset() {
        buffer.clear()
        sampleCount = 0
    }

    private fun appendTrailPoint(x: Float, y: Float) {
        maxMoveDistance = max(maxMoveDistance, distance(downX, downY, x, y))
        if (maxMoveDistance > tapSlop) {
            drawingGesture = true
            trailPath.lineTo(x, y)
            invalidate()
        }
    }

    private fun distance(x0: Float, y0: Float, x1: Float, y1: Float): Float {
        val dx = x1 - x0
        val dy = y1 - y0
        return kotlin.math.sqrt(dx * dx + dy * dy)
    }

    private fun appendAdjusted(x: Float, y: Float, action: Int, pointerId: Int, eventTime: Long) {
        append(x - gridInsetLeft, y - gridInsetTop, action, pointerId, eventTime)
    }

    private fun append(x: Float, y: Float, action: Int, pointerId: Int, eventTime: Long) {
        if (sampleCount >= MaxSamples) {
            buffer.position((MaxSamples - 1) * MotionSampleBytes)
            writeSample(x, y, action, pointerId, eventTime)
            buffer.position(MaxSamples * MotionSampleBytes)
            return
        }
        writeSample(x, y, action, pointerId, eventTime)
        sampleCount += 1
    }

    private fun writeSample(x: Float, y: Float, action: Int, pointerId: Int, eventTime: Long) {
        buffer.putFloat(x)
        buffer.putFloat(y)
        buffer.putInt(action)
        buffer.putInt(pointerId)
        buffer.putLong(eventTime)
    }
}
