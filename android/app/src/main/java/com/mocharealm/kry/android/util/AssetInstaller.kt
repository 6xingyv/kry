package com.mocharealm.kry.android.util

import android.content.Context
import android.content.pm.PackageManager
import android.content.res.AssetManager
import android.os.Build
import java.io.File

object AssetInstaller {
    private const val CORE_STAMP_FILE = "installed_version"
    private const val LM_STAMP_FILE = "installed_lm_version"

    /**
     * Install the CORE assets the engine needs to be usable: language packs +
     * observation model (~14MB). The big LM weights are installed separately and
     * lazily by [ensureLmInstalled] so the keyboard is responsive fast on first run.
     */
    fun ensureInstalled(context: Context): File {
        val root = File(context.filesDir, "kry-assets")
        val stamp = File(root, CORE_STAMP_FILE)
        val version = getVersionCode(context).toString()
        if (stamp.exists() && stamp.readText() == version) return root

        copyTree(context.assets, "language-packs", root)
        copyTree(context.assets, "observation-models", root)

        stamp.parentFile?.mkdirs()
        stamp.writeText(version)
        return root
    }

    /**
     * Install the LM weights (zh `lm/` + en `lm-en/`, ~42MB). Called on a
     * background thread AFTER the engine is already created, then loaded via
     * `nativeLoadLm`. Heavy; never call on the UI path.
     */
    fun ensureLmInstalled(context: Context): File {
        val root = File(context.filesDir, "kry-assets")
        val stamp = File(root, LM_STAMP_FILE)
        val version = getVersionCode(context).toString()
        if (stamp.exists() && stamp.readText() == version) return root

        copyTree(context.assets, "lm", root)
        copyTree(context.assets, "lm-en", root)

        stamp.parentFile?.mkdirs()
        stamp.writeText(version)
        return root
    }

    private fun getVersionCode(context: Context): Long {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            context.packageManager
                .getPackageInfo(context.packageName, 0)
                .longVersionCode
        } else {
            @Suppress("DEPRECATION")
            context.packageManager
                .getPackageInfo(context.packageName, 0)
                .versionCode.toLong()
        }
    }

    private fun copyTree(assetManager: AssetManager, path: String, root: File) {
        val children = assetManager.list(path).orEmpty()
        val target = File(root, path)
        if (children.isEmpty()) {
            target.parentFile?.mkdirs()
            assetManager.open(path).use { input ->
                if (target.exists() && target.length() == input.available().toLong()) return
                target.outputStream().use { output -> input.copyTo(output) }
            }
            return
        }
        target.mkdirs()
        for (child in children) {
            copyTree(assetManager, "$path/$child", root)
        }
    }
}

