use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use engine_core::ImeEngine;
use geometry_core::Point;
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass, JString};
use jni::sys::{jfloat, jint, jlong, jstring};

const GEOMETRY_HEIGHT: f32 = 0.83;
const MAX_GESTURE_POINTS: usize = 48;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct MotionSample {
    x: f32,
    y: f32,
    action: i32,
    pointer_id: i32,
    event_time_millis: i64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct TapPointSample {
    x: f32,
    y: f32,
}

pub struct AndroidEngine {
    inner: ImeEngine,
}

impl AndroidEngine {
    fn new(
        language_pack_root: &Path,
        observation_pack_root: &Path,
        lm_root: Option<&Path>,
        profile_id: &str,
    ) -> Result<Self, String> {
        let mut inner = match profile_id {
            "en-qwerty" => {
                ImeEngine::en_qwerty_from_artifacts(language_pack_root, observation_pack_root)
                    .map_err(|err| format!("failed to load en engine: {err}"))?
            }
            _ => ImeEngine::zh_qwerty_from_artifacts(language_pack_root, observation_pack_root)
                .map_err(|err| format!("failed to load zh engine: {err}"))?,
        };
        if let Some(lm_root) = lm_root {
            if let Err(err) = inner.load_lm_from_dir(lm_root) {
                // LM loading is optional; fall back to NullLanguageModel.
                eprintln!("kry: failed to load LM from {}: {err}", lm_root.display());
            }
        }
        Ok(Self { inner })
    }

    fn decode_gesture(
        &self,
        samples: &[MotionSample],
        view_width: f32,
        view_height: f32,
    ) -> String {
        if view_width <= 0.0 || view_height <= 0.0 {
            return String::new();
        }
        let points = samples
            .iter()
            .map(|sample| {
                Point::new(
                    (sample.x / view_width).clamp(0.0, 1.0),
                    (sample.y / view_height * GEOMETRY_HEIGHT).clamp(0.0, GEOMETRY_HEIGHT),
                )
            })
            .collect::<Vec<_>>();
        let points = downsample_points(points, MAX_GESTURE_POINTS);
        if points.len() < 2 {
            return String::new();
        }
        let candidates = self.inner.decode_gesture_trace(&points, 8);
        encode_candidates(&candidates)
    }

    fn decode_taps(&self, samples: &[TapPointSample], view_width: f32, view_height: f32) -> String {
        if view_width <= 0.0 || view_height <= 0.0 {
            return String::new();
        }
        let points = samples
            .iter()
            .map(|sample| {
                Point::new(
                    (sample.x / view_width).clamp(0.0, 1.0),
                    (sample.y / view_height * GEOMETRY_HEIGHT).clamp(0.0, GEOMETRY_HEIGHT),
                )
            })
            .collect::<Vec<_>>();
        if points.is_empty() {
            return String::new();
        }
        let composed = self.inner.decode_taps_composed(&points);
        if !composed.sentences.is_empty() {
            encode_sentence_candidates(&composed.sentences)
        } else {
            let candidates = self.inner.decode_taps(&points);
            encode_candidates(&candidates)
        }
    }
}

fn downsample_points(points: Vec<Point>, limit: usize) -> Vec<Point> {
    if limit == 0 || points.len() <= limit {
        return points;
    }
    if limit == 1 {
        return points.last().copied().into_iter().collect();
    }
    (0..limit)
        .map(|index| {
            let src = index as f32 * (points.len() - 1) as f32 / (limit - 1) as f32;
            points[src.round() as usize]
        })
        .collect()
}

fn encode_candidates(candidates: &decoder_core::CandidateList) -> String {
    let mut out = String::new();
    for candidate in candidates.candidates.iter().take(12) {
        out.push_str(&candidate.text.replace(['\t', '\n'], " "));
        out.push('\t');
        out.push_str(&candidate.reading.replace(['\t', '\n'], " "));
        out.push('\t');
        out.push_str(&format!("{:.4}", candidate.score));
        out.push('\n');
    }
    out
}

fn encode_sentence_candidates(candidates: &[decoder_core::SentenceCandidate]) -> String {
    let mut out = String::new();
    for candidate in candidates.iter().take(12) {
        out.push_str(&candidate.total_text.replace(['\t', '\n'], " "));
        out.push('\t');
        let reading = candidate
            .words
            .iter()
            .map(|word| word.reading.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        out.push_str(&reading.replace(['\t', '\n'], " "));
        out.push('\t');
        out.push_str(&format!("{:.4}", candidate.total_score));
        out.push('\n');
    }
    out
}

fn jstring_to_string(env: &mut JNIEnv<'_>, value: JString<'_>) -> Result<String, String> {
    env.get_string(&value)
        .map(|s| s.to_string_lossy().into_owned())
        .map_err(|err| err.to_string())
}

fn throw(env: &mut JNIEnv<'_>, message: impl AsRef<str>) {
    let _ = env.throw_new("java/lang/IllegalStateException", message.as_ref());
}

fn ptr_from_handle<'a>(handle: jlong) -> Option<&'a mut AndroidEngine> {
    if handle == 0 {
        return None;
    }
    let ptr = handle as *mut AndroidEngine;
    unsafe { ptr.as_mut() }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_mocharealm_kry_android_nativebridge_KryNative_nativeCreate(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    language_pack_root: JString<'_>,
    observation_pack_root: JString<'_>,
    lm_root: JString<'_>,
    profile_id: JString<'_>,
) -> jlong {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let language_pack_root = jstring_to_string(&mut env, language_pack_root)?;
        let observation_pack_root = jstring_to_string(&mut env, observation_pack_root)?;
        let lm_root = jstring_to_string(&mut env, lm_root)?;
        let profile_id = jstring_to_string(&mut env, profile_id)?;
        let lm_path = if lm_root.is_empty() {
            None
        } else {
            Some(Path::new(&lm_root).to_path_buf())
        };
        AndroidEngine::new(
            Path::new(&language_pack_root),
            Path::new(&observation_pack_root),
            lm_path.as_deref(),
            &profile_id,
        )
    }));
    match result {
        Ok(Ok(engine)) => Box::into_raw(Box::new(engine)) as jlong,
        Ok(Err(message)) => {
            throw(&mut env, message);
            0
        }
        Err(_) => {
            throw(&mut env, "native engine creation panicked");
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_mocharealm_kry_android_nativebridge_KryNative_nativeDestroy(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) {
    if handle != 0 {
        unsafe {
            drop(Box::from_raw(handle as *mut AndroidEngine));
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_mocharealm_kry_android_nativebridge_KryNative_nativeAcceptCandidate(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    text: JString<'_>,
    reading: JString<'_>,
) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let engine =
            ptr_from_handle(handle).ok_or_else(|| "native engine is not initialized".to_owned())?;
        let text = jstring_to_string(&mut env, text)?;
        let reading = jstring_to_string(&mut env, reading)?;
        engine.inner.accept_swipe_candidate(&text);
        // Learn into the adaptive user dictionary (reading -> text).
        engine.inner.learn_commit(&reading, &text);
        Ok::<_, String>(())
    }));
    match result {
        Ok(Ok(())) => {}
        Ok(Err(message)) => throw(&mut env, message),
        Err(_) => throw(&mut env, "native accept candidate panicked"),
    }
}

/// Set the session context from the editor's surrounding text (the host reads
/// `getTextBeforeCursor` on focus / cursor move). Feeds both the tap-decode
/// `TextContext` energy term and the swipe-LM reranker history.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_mocharealm_kry_android_nativebridge_KryNative_nativeSetContext(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    text: JString<'_>,
) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let engine =
            ptr_from_handle(handle).ok_or_else(|| "native engine is not initialized".to_owned())?;
        let text = jstring_to_string(&mut env, text)?;
        engine.inner.set_editor_context(&text);
        Ok::<_, String>(())
    }));
    match result {
        Ok(Ok(())) => {}
        Ok(Err(message)) => throw(&mut env, message),
        Err(_) => throw(&mut env, "native set context panicked"),
    }
}

/// Serialize the learned user dictionary as `reading\ttext\tcount` lines so the
/// host can persist it to disk.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_mocharealm_kry_android_nativebridge_KryNative_nativeExportUserDict<
    'local,
>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> JString<'local> {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let engine =
            ptr_from_handle(handle).ok_or_else(|| "native engine is not initialized".to_owned())?;
        let dump = engine
            .inner
            .user_dictionary()
            .into_iter()
            .map(|(reading, text, count)| format!("{reading}\t{text}\t{count}"))
            .collect::<Vec<_>>()
            .join("\n");
        Ok::<_, String>(dump)
    }));
    match result {
        Ok(Ok(dump)) => env
            .new_string(dump)
            .unwrap_or_else(|_| env.new_string("").unwrap()),
        Ok(Err(message)) => {
            throw(&mut env, message);
            env.new_string("").unwrap()
        }
        Err(_) => {
            throw(&mut env, "native export user dict panicked");
            env.new_string("").unwrap()
        }
    }
}

/// Restore a persisted user dictionary from `reading\ttext\tcount` lines.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_mocharealm_kry_android_nativebridge_KryNative_nativeImportUserDict(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    data: JString<'_>,
) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let engine =
            ptr_from_handle(handle).ok_or_else(|| "native engine is not initialized".to_owned())?;
        let data = jstring_to_string(&mut env, data)?;
        let entries = data.lines().filter_map(|line| {
            let mut parts = line.split('\t');
            let reading = parts.next()?.to_owned();
            let text = parts.next()?.to_owned();
            let count = parts.next()?.parse::<u32>().ok()?;
            Some((reading, text, count))
        });
        engine.inner.load_user_dictionary(entries);
        Ok::<_, String>(())
    }));
    match result {
        Ok(Ok(())) => {}
        Ok(Err(message)) => throw(&mut env, message),
        Err(_) => throw(&mut env, "native import user dict panicked"),
    }
}

/// Lazily install a language model into an already-created engine (so engine
/// creation stays fast). Fail-safe: if the LM can't load, the engine keeps its
/// NullLanguageModel — decode still works, just without the context re-ranker.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_mocharealm_kry_android_nativebridge_KryNative_nativeLoadLm(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    lm_root: JString<'_>,
) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let engine =
            ptr_from_handle(handle).ok_or_else(|| "native engine is not initialized".to_owned())?;
        let lm_root = jstring_to_string(&mut env, lm_root)?;
        if !lm_root.is_empty() {
            if let Err(err) = engine.inner.load_lm_from_dir(&lm_root) {
                eprintln!("kry: nativeLoadLm failed: {err}");
            }
        }
        Ok::<_, String>(())
    }));
    match result {
        Ok(Ok(())) => {}
        Ok(Err(message)) => throw(&mut env, message),
        Err(_) => throw(&mut env, "native load lm panicked"),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_mocharealm_kry_android_nativebridge_KryNative_nativeResetSwipe(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let engine =
            ptr_from_handle(handle).ok_or_else(|| "native engine is not initialized".to_owned())?;
        engine.inner.reset_swipe_session();
        Ok::<_, String>(())
    }));
    match result {
        Ok(Ok(())) => {}
        Ok(Err(message)) => throw(&mut env, message),
        Err(_) => throw(&mut env, "native reset swipe panicked"),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_mocharealm_kry_android_nativebridge_KryNative_nativeDecodeGesture(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    buffer: JByteBuffer<'_>,
    sample_count: jint,
    view_width: jfloat,
    view_height: jfloat,
) -> jstring {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let engine =
            ptr_from_handle(handle).ok_or_else(|| "native engine is not initialized".to_owned())?;
        if sample_count <= 0 {
            return Ok(String::new());
        }
        let address = env
            .get_direct_buffer_address(&buffer)
            .map_err(|err| err.to_string())?;
        let capacity = env
            .get_direct_buffer_capacity(&buffer)
            .map_err(|err| err.to_string())?;
        let requested_bytes = sample_count as usize * std::mem::size_of::<MotionSample>();
        if requested_bytes > capacity {
            return Err(format!(
                "gesture buffer too small: requested {requested_bytes}, capacity {capacity}"
            ));
        }
        let samples = unsafe {
            std::slice::from_raw_parts(address as *const MotionSample, sample_count as usize)
        };
        Ok(engine.decode_gesture(samples, view_width, view_height))
    }));

    let output = match result {
        Ok(Ok(output)) => output,
        Ok(Err(message)) => {
            throw(&mut env, message);
            String::new()
        }
        Err(_) => {
            throw(&mut env, "native gesture decode panicked");
            String::new()
        }
    };
    env.new_string(output)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_mocharealm_kry_android_nativebridge_KryNative_nativeDecodeTaps(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    buffer: JByteBuffer<'_>,
    point_count: jint,
    view_width: jfloat,
    view_height: jfloat,
) -> jstring {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let engine =
            ptr_from_handle(handle).ok_or_else(|| "native engine is not initialized".to_owned())?;
        if point_count <= 0 {
            return Ok(String::new());
        }
        let address = env
            .get_direct_buffer_address(&buffer)
            .map_err(|err| err.to_string())?;
        let capacity = env
            .get_direct_buffer_capacity(&buffer)
            .map_err(|err| err.to_string())?;
        let requested_bytes = point_count as usize * std::mem::size_of::<TapPointSample>();
        if requested_bytes > capacity {
            return Err(format!(
                "tap buffer too small: requested {requested_bytes}, capacity {capacity}"
            ));
        }
        let samples = unsafe {
            std::slice::from_raw_parts(address as *const TapPointSample, point_count as usize)
        };
        Ok(engine.decode_taps(samples, view_width, view_height))
    }));

    let output = match result {
        Ok(Ok(output)) => output,
        Ok(Err(message)) => {
            throw(&mut env, message);
            String::new()
        }
        Err(_) => {
            throw(&mut env, "native tap decode panicked");
            String::new()
        }
    };
    env.new_string(output)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_sample_layout_matches_android_writer() {
        assert_eq!(std::mem::size_of::<MotionSample>(), 24);
        assert_eq!(std::mem::align_of::<MotionSample>(), 8);
    }

    #[test]
    fn tap_point_sample_layout_matches_android_writer() {
        assert_eq!(std::mem::size_of::<TapPointSample>(), 8);
        assert_eq!(std::mem::align_of::<TapPointSample>(), 4);
    }
}
