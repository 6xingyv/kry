import com.android.utils.usLocaleDecapitalize
import org.gradle.internal.os.OperatingSystem
import java.io.File
import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
}

fun getGitCommitHash(): String {
    return try {
        val process = ProcessBuilder("git", "rev-parse", "--short", "HEAD")
            .directory(rootProject.projectDir.parentFile)
            .redirectErrorStream(true)
            .start()
        process.inputStream.bufferedReader().readText().trim()
    } catch (e: Exception) {
        "unknown"
    }
}

val localProperties = Properties().apply {
    val file = rootProject.file("local.properties")
    if (file.exists()) file.inputStream().use { load(it) }
}

android {
    namespace = "com.mocharealm.kry.android"
    compileSdk {
        version = release(37)
    }

    defaultConfig {
        applicationId = "com.mocharealm.kry.android"
        minSdk = 34
        targetSdk = 37
        versionCode = libs.versions.appVersionCode.get().toInt()
        versionName = libs.versions.appVersionName.get()
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        buildConfigField("String", "GIT_COMMIT_HASH", "\"${getGitCommitHash()}\"")
    }

    sourceSets {
        getByName("main") {
            jniLibs.directories.add("src/main/jniLibs")
            assets.directories.add(rootProject.file("../assets").absolutePath)
        }
    }

    androidResources {
        ignoreAssetsPatterns += listOf(
            "archive",
            "model-q8.safetensors",
            "gesture_templates.bin",
            "*.md",
            "*.tsv",
        )
    }

    buildTypes {
        debug {
            signingConfig = signingConfigs.getByName("debug")
        }
        release {
            isMinifyEnabled = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    splits {
        abi {
            isEnable = true
            isUniversalApk = false
            reset()
            include("armeabi-v7a", "arm64-v8a", "x86_64")
        }
    }

    dependenciesInfo {
        includeInApk = false
        includeInBundle = false
    }

    ndkVersion = "29.0.14206865"
}

val cargoCommand = if (OperatingSystem.current().isWindows) "cargo.exe" else "cargo"
val rustOutputDir = layout.projectDirectory.dir("src/main/jniLibs")
val sdkDir = localProperties.getProperty("sdk.dir") ?: providers.environmentVariable("ANDROID_HOME").getOrElse("")
val ndkHome = providers.environmentVariable("ANDROID_NDK_HOME")
    .orElse(
        if (sdkDir.isNotBlank()) {
            File(sdkDir, "ndk/29.0.14206865").absolutePath
        } else {
            ""
        }
    )

tasks.register<Exec>("cargoBuildRust") {
    workingDir = rootProject.projectDir.parentFile
    environment("ANDROID_NDK_HOME", ndkHome.get())
    commandLine(
        cargoCommand,
        "ndk",
        "-t", "armeabi-v7a",
        "-t", "arm64-v8a",
        "-t", "x86_64",
        "-o", rustOutputDir.asFile.absolutePath,
        "build",
        "-p", "android-ffi",
        "--release"
    )
    inputs.files(
        fileTree(rootProject.file("../crates")) {
            include("**/*.rs")
            include("**/Cargo.toml")
        },
        rootProject.file("../Cargo.toml"),
        rootProject.file("../Cargo.lock")
    )
    outputs.dir(rustOutputDir)
}

tasks.named("preBuild") {
    dependsOn("cargoBuildRust")
}

base {
    archivesName.set(
        "${rootProject.name.usLocaleDecapitalize()}-${libs.versions.appVersionName.get().replace(" ", "-")}"
    )
}

dependencies {
    implementation(project(":emojipicker"))

    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.activity.compose)

    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.foundation)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.compose.material3.window.size)
    implementation(libs.androidx.compose.ui.graphics)
    implementation(libs.androidx.compose.ui.tooling.preview)

    implementation(libs.androidx.navigation3.runtime)
    implementation(libs.androidx.navigation3.ui)
    implementation(libs.androidx.navigationevent)
    implementation(libs.materialKolor)

    implementation(libs.kotlinx.serialization.core)

    testImplementation(libs.junit)
    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.espresso.core)
    debugImplementation(libs.androidx.compose.ui.tooling)
    debugImplementation(libs.androidx.compose.ui.test.manifest)
}
