package com.mocharealm.kry.android.nativebridge

import java.nio.ByteBuffer

object KryNative {
    init {
        System.loadLibrary("kry_android")
    }

    @JvmStatic
    external fun nativeCreate(
        languagePackRoot: String,
        observationPackRoot: String,
        lmRoot: String,
        profileId: String,
    ): Long

    @JvmStatic
    external fun nativeDestroy(handle: Long)

    @JvmStatic
    external fun nativeAcceptCandidate(handle: Long, text: String, reading: String)

    @JvmStatic
    external fun nativeSetContext(handle: Long, text: String)

    @JvmStatic
    external fun nativeExportUserDict(handle: Long): String

    @JvmStatic
    external fun nativeImportUserDict(handle: Long, data: String)

    @JvmStatic
    external fun nativeLoadLm(handle: Long, lmRoot: String)

    @JvmStatic
    external fun nativeResetSwipe(handle: Long)

    @JvmStatic
    external fun nativeDecodeGesture(
        handle: Long,
        buffer: ByteBuffer,
        sampleCount: Int,
        viewWidth: Float,
        viewHeight: Float,
    ): String

    @JvmStatic
    external fun nativeDecodeTaps(
        handle: Long,
        buffer: ByteBuffer,
        pointCount: Int,
        viewWidth: Float,
        viewHeight: Float,
    ): String
}
