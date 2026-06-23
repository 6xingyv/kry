package com.mocharealm.kry.android.nativebridge

import android.content.Context
import android.util.Log
import com.mocharealm.kry.android.settings.KeyboardProfileStore
import com.mocharealm.kry.android.util.AssetInstaller
import java.io.Closeable
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

private const val TapPointBytes = 8
private const val TAG = "KryEngine"

data class TapPoint(
    val x: Float,
    val y: Float,
)

class KryEngine : Closeable {
    private val handle = AtomicLong(0L)
    // Guards against concurrent/redundant loads (initialize is called from several
    // IME lifecycle callbacks). Only one load runs at a time.
    private val loading = AtomicBoolean(false)

    @Volatile
    var activeProfileId: String = KeyboardProfileStore.ProfileZhQwerty
        private set

    /**
     * Whether the native engine has been successfully initialized.
     */
    val isReady: Boolean get() = handle.get() != 0L

    /**
     * Heavy initialization: asset installation + native engine creation. Call from
     * a background thread (e.g. `Dispatchers.IO`).
     *
     * CRITICAL: the slow work (asset copy, native engine build) runs WITHOUT the
     * decode monitor — only the brief handle swap is synchronized. Otherwise
     * `decodeTaps` (called on the UI thread) would block on this for seconds while
     * loading, freezing input → ANR → the system kills the IME.
     *
     * Returns true if it loaded (or was already loaded for this profile).
     */
    fun initialize(
        context: Context,
        profileId: String = KeyboardProfileStore.activeProfileId(context),
    ): Boolean {
        val target = KeyboardProfileStore.profileForId(profileId).id
        if (isReady && activeProfileId == target) return true
        // A load is already running — let it finish; the caller re-checks the
        // active profile afterwards and reloads if a different one is wanted.
        if (!loading.compareAndSet(false, true)) return false
        try {
            val root = AssetInstaller.ensureInstalled(context)
            // Create WITHOUT the LM (empty lmRoot) so the engine is usable fast;
            // the ~42MB LM is installed lazily by loadLm() afterwards.
            val created = KryNative.nativeCreate(
                languagePackRoot = root.resolve("language-packs").absolutePath,
                observationPackRoot = root.resolve("observation-models/geometry-phone-10col/qwerty").absolutePath,
                lmRoot = "",
                profileId = target,
            )
            synchronized(this) {
                val previous = handle.getAndSet(created)
                activeProfileId = target
                if (previous != 0L) KryNative.nativeDestroy(previous)
            }
            return true
        } catch (t: Throwable) {
            // A load failure (missing asset, native error, OOM) must NEVER crash the
            // IME process — that kills the keyboard. Log and stay not-ready; the
            // surface still shows and decode returns empty until a later reload.
            Log.e(TAG, "engine initialize failed for profile $target", t)
            return false
        } finally {
            loading.set(false)
        }
    }

    /**
     * Install the LM into the live engine. Heavy — call from a background thread,
     * AFTER [initialize]. The asset copy runs outside the decode lock; only the
     * brief native load is synchronized. Fail-safe: a failure just disables the
     * context re-ranker, decode still works.
     */
    fun loadLm(context: Context) {
        if (!isReady) return
        val root = AssetInstaller.ensureLmInstalled(context)
        synchronized(this) {
            val current = handle.get()
            if (current != 0L) KryNative.nativeLoadLm(current, root.resolve("lm").absolutePath)
        }
    }

    @Synchronized
    fun decodeGesture(
        buffer: ByteBuffer,
        sampleCount: Int,
        viewWidth: Float,
        viewHeight: Float,
    ): List<Candidate> {
        val current = handle.get()
        if (current == 0L || sampleCount <= 0) return emptyList()
        buffer.position(0)
        return parseCandidates(
            KryNative.nativeDecodeGesture(
                handle = current,
                buffer = buffer,
                sampleCount = sampleCount,
                viewWidth = viewWidth,
                viewHeight = viewHeight,
            )
        )
    }

    @Synchronized
    fun decodeTaps(
        points: List<TapPoint>,
        viewWidth: Float,
        viewHeight: Float,
    ): List<Candidate> {
        val current = handle.get()
        if (current == 0L || points.isEmpty()) return emptyList()
        val buffer = ByteBuffer
            .allocateDirect(points.size * TapPointBytes)
            .order(ByteOrder.nativeOrder())
        for (point in points) {
            buffer.putFloat(point.x)
            buffer.putFloat(point.y)
        }
        buffer.position(0)
        return parseCandidates(
            KryNative.nativeDecodeTaps(
                handle = current,
                buffer = buffer,
                pointCount = points.size,
                viewWidth = viewWidth,
                viewHeight = viewHeight,
            )
        )
    }

    @Synchronized
    fun acceptCandidate(text: String, reading: String = "") {
        val current = handle.get()
        if (current != 0L) KryNative.nativeAcceptCandidate(current, text, reading)
    }

    /**
     * Set the session context from the editor's surrounding text (text before the
     * cursor). Call from a background thread; cheap but takes the engine lock.
     */
    @Synchronized
    fun setContext(text: String) {
        val current = handle.get()
        if (current != 0L) KryNative.nativeSetContext(current, text)
    }

    /** Serialize the learned user dictionary for persistence. */
    @Synchronized
    fun exportUserDict(): String {
        val current = handle.get()
        return if (current != 0L) KryNative.nativeExportUserDict(current) else ""
    }

    /** Restore a persisted user dictionary (call once after engine creation). */
    @Synchronized
    fun importUserDict(data: String) {
        val current = handle.get()
        if (current != 0L && data.isNotEmpty()) KryNative.nativeImportUserDict(current, data)
    }

    @Synchronized
    fun resetSwipe() {
        val current = handle.get()
        if (current != 0L) KryNative.nativeResetSwipe(current)
    }

    @Synchronized
    override fun close() {
        val current = handle.getAndSet(0L)
        if (current != 0L) {
            KryNative.nativeDestroy(current)
        }
    }

    private fun parseCandidates(raw: String): List<Candidate> {
        if (raw.isBlank()) return emptyList()
        return raw.lineSequence()
            .filter { it.isNotBlank() }
            .mapNotNull { line ->
                val parts = line.split('\t')
                if (parts.size < 3) return@mapNotNull null
                Candidate(
                    text = parts[0],
                    reading = parts[1],
                    score = parts[2].toFloatOrNull() ?: 0f,
                )
            }
            .toList()
    }
}
