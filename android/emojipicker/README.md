# :emojipicker

A self-contained **Jetpack Compose + Material 3** emoji picker — a port of AndroidX
`emoji2-emojipicker` (which is RecyclerView/`View`-based) to Compose, with the emoji data
taken **directly from Unicode's official `emoji-test.txt`**.

## Features
- Category tabs (`PrimaryScrollableTabRow`) synced to grid scroll, with a "Recently used" tab.
- Vertically scrollable emoji grid (`LazyVerticalGrid`) with category section headers.
- Long-press **skin-tone variant** popup (codepoints U+1F3FB–U+1F3FF folded under each base).
- Recently-used list + sticky per-emoji skin-tone choice (persisted in `SharedPreferences`).
- Renderability filtering (`PaintCompat.hasGlyph`) so unsupported glyphs ("tofu") are hidden.

## Files
```
build.gradle.kts                              # com.android.library + kotlin.compose, compileSdk 37
src/main/AndroidManifest.xml
src/main/java/com/mocharealm/kry/emojipicker/
    EmojiData.kt      # parses res/raw/emoji_test.txt → categories + skin-tone variants (+ render filter)
    EmojiStore.kt     # recent emojis + sticky skin tone (SharedPreferences)
    EmojiPicker.kt    # the @Composable EmojiPicker(...) UI (Material 3)
src/main/res/raw/emoji_test.txt               # Unicode emoji-test.txt (the data source)
src/main/res/drawable/ic_emoji_*_24px.xml     # category + recent + backspace icons (Material Symbols)
```

## Dependencies
Uses the project version catalog (`gradle/libs.versions.toml`):
`androidx.core:core-ktx`, Compose `ui`/`foundation`/`ui-graphics`, `androidx.compose.material3`.
Plugins: `com.android.library`, `org.jetbrains.kotlin.plugin.compose`. `minSdk 34`, `compileSdk 37`
(Compose 1.12.x, pulled transitively by material3 1.5.0-alpha22, requires compileSdk 37).

To consume from another project, declare those catalog aliases (or hardcode the versions in
`build.gradle.kts`) and add `include(":emojipicker")` to `settings.gradle.kts`.

## Usage
```kotlin
EmojiPicker(
    onEmojiPicked = { emoji -> inputConnection.commitText(emoji, 1) },
    onBackspace   = { /* delete */ },
    onClose       = { /* leave the panel, e.g. show the keyboard again */ },
    columns       = 8,            // optional, default 8
)
```
The composable uses the ambient `MaterialTheme` — wrap it in your theme (dynamic color works).

## Updating the emoji set
Re-fetch and replace the bundled data; the parser tracks Unicode's `# group:` sections and
`fully-qualified` entries automatically:
```sh
curl -L https://www.unicode.org/Public/emoji/latest/emoji-test.txt \
    -o src/main/res/raw/emoji_test.txt
```

## Attribution / license
- Emoji data: Unicode® `emoji-test.txt` (Unicode license / public data).
- Code structure ported from AndroidX `emoji2-emojipicker` — Apache License 2.0.
- Icons: Google Material Symbols — Apache License 2.0.
