use super::*;
use data_core::{Lexicon, ObservationModelPack};
use geometry_core::{GeometryLayout, SlotId, SlotObservation};
use geometry_phone_10col::Phone10ColGeometry;
use keymap_latin_qwerty::LatinQwertyKeyMap;
use keymap_latin_spanish::LatinSpanishKeyMap;
use keymap_ru_jcuken::RuJcukenKeyMap;
use observation_core::{InputSource, ObservationChunk, ObservationPoint, RawInputEvent};

fn exact_lattice(symbols: &str) -> SlotLattice {
    let keymap = LatinQwertyKeyMap::new();
    SlotLattice::new(
        symbols
            .chars()
            .map(|ch| {
                vec![SlotObservation {
                    slot_id: keymap.slot_for_symbol(ch).unwrap(),
                    cost: 0.0,
                }]
            })
            .collect(),
    )
}

fn exact_spanish_lattice(symbols: &str) -> SlotLattice {
    let keymap = LatinSpanishKeyMap::new();
    SlotLattice::new(
        symbols
            .chars()
            .map(|ch| {
                vec![SlotObservation {
                    slot_id: keymap.slot_for_symbol(ch).unwrap(),
                    cost: 0.0,
                }]
            })
            .collect(),
    )
}

fn exact_ru_lattice(symbols: &str) -> SlotLattice {
    let keymap = RuJcukenKeyMap::new();
    SlotLattice::new(
        symbols
            .chars()
            .map(|ch| {
                vec![SlotObservation {
                    slot_id: keymap.slot_for_symbol(ch).unwrap(),
                    cost: 0.0,
                }]
            })
            .collect(),
    )
}

fn dense_trace_through(symbols: &str) -> Vec<Point> {
    let geometry = Phone10ColGeometry::new();
    let keymap = LatinQwertyKeyMap::new();
    let centers: Vec<Point> = symbols
        .chars()
        .map(|ch| {
            let slot_id = keymap.slot_for_symbol(ch).unwrap();
            geometry.slot(&slot_id).unwrap().center()
        })
        .collect();
    if centers.len() <= 1 {
        return centers;
    }
    let points_per_segment = 8;
    let mut trace = vec![centers[0]];
    for i in 1..centers.len() {
        let from = centers[i - 1];
        let to = centers[i];
        for j in 1..=points_per_segment {
            let t = j as f32 / points_per_segment as f32;
            trace.push(Point::new(
                from.x + (to.x - from.x) * t,
                from.y + (to.y - from.y) * t,
            ));
        }
    }
    trace
}

fn exact_points(symbols: &str) -> Vec<Point> {
    let geometry = Phone10ColGeometry::new();
    let keymap = LatinQwertyKeyMap::new();
    symbols
        .chars()
        .map(|ch| {
            let slot_id = keymap.slot_for_symbol(ch).unwrap();
            geometry.slot(&slot_id).unwrap().center()
        })
        .collect()
}

fn workspace_asset_path(path: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(path)
}

fn observation_point_for_symbol(engine: &ImeEngine, symbol: char, t_ms: u64) -> ObservationPoint {
    let keymap = LatinQwertyKeyMap::new();
    let slot_id = keymap.slot_for_symbol(symbol).unwrap();
    let center = engine.geometry.slot(&slot_id).unwrap().center();
    ObservationPoint::new(center.x, center.y, t_ms)
}

#[test]
fn chinese_context_pulls_xi_an() {
    let lexicon = Lexicon::new([
        data_core::LexiconEntry::new("先", "xian", 100.0),
        data_core::LexiconEntry::new("西安", "xi an", 100.0),
        data_core::LexiconEntry::new("去西安", "qu xi an", 100.0),
    ]);
    let mut cache = ContextPredictiveCache::empty();
    add_zh_lexicon_continuations(&mut cache, "zh-hans-pinyin-full", &lexicon, 10);
    let mut engine = ImeEngine::zh_qwerty_with_lexicons(lexicon, Lexicon::new([]), false);
    engine.context_compiler = ContextCompiler::with_base_cache(cache);
    engine.set_committed_context("我想去");
    let candidates = engine.decode_lattice(&exact_lattice("xian"));
    let top = candidates.top().unwrap();
    assert_eq!(top.text, "西安");
    assert_eq!(top.reading, "xi an");
    assert_eq!(top.boundary.segments, ["xi", "an"]);
}

#[test]
fn editor_context_drives_decode_and_swipe_history() {
    // set_editor_context (the host's getTextBeforeCursor path) must feed BOTH the
    // tap-decode TextContext (same biasing as set_committed_context) AND the swipe-LM
    // history (swipe_accepted_text), and REPLACE rather than append.
    let lexicon = Lexicon::new([
        data_core::LexiconEntry::new("先", "xian", 100.0),
        data_core::LexiconEntry::new("西安", "xi an", 100.0),
        data_core::LexiconEntry::new("去西安", "qu xi an", 100.0),
    ]);
    let mut cache = ContextPredictiveCache::empty();
    add_zh_lexicon_continuations(&mut cache, "zh-hans-pinyin-full", &lexicon, 10);
    let mut engine = ImeEngine::zh_qwerty_with_lexicons(lexicon, Lexicon::new([]), false);
    engine.context_compiler = ContextCompiler::with_base_cache(cache);

    engine.accept_swipe_candidate("我");
    engine.set_editor_context("我想去"); // replaces the appended "我", not append
    assert_eq!(engine.swipe_session_text(), "我想去");

    let top = engine.decode_lattice(&exact_lattice("xian")).top().unwrap().clone();
    assert_eq!(top.text, "西安");

    // Empty editor (cleared field) wipes the context so isolated words fall back.
    engine.set_editor_context("");
    assert_eq!(engine.swipe_session_text(), "");
}

#[test]
fn pause_boundary_hint_pulls_xi_an_without_context() {
    let engine = ImeEngine::zh_qwerty_with_lexicons(
        Lexicon::new([
            data_core::LexiconEntry::new("先", "xian", 100.0),
            data_core::LexiconEntry::new("西安", "xi an", 100.0),
        ]),
        Lexicon::new([]),
        false,
    );
    let candidates = engine.decode_lattice_with_boundary_hints(&exact_lattice("xian"), &[2]);
    let top = candidates.top().unwrap();
    assert_eq!(top.text, "西安");
    assert_eq!(top.boundary.segments, ["xi", "an"]);
}

#[test]
fn streaming_trace_uses_pause_as_boundary_evidence() {
    let mut engine = ImeEngine::zh_qwerty_with_lexicons(
        Lexicon::new([
            data_core::LexiconEntry::new("先", "xian", 100.0),
            data_core::LexiconEntry::new("西安", "xi an", 100.0),
        ]),
        Lexicon::new([]),
        false,
    );
    let first = ObservationChunk::new(
        InputSource::Trace,
        vec![RawInputEvent::Trace(vec![
            observation_point_for_symbol(&engine, 'x', 0),
            observation_point_for_symbol(&engine, 'i', 20),
        ])],
    );
    let candidates = engine.feed_observation_chunk(&first);
    assert!(candidates.top().is_some());

    let second = ObservationChunk::new(
        InputSource::Trace,
        vec![
            RawInputEvent::Pause { duration_ms: 220 },
            RawInputEvent::Trace(vec![
                observation_point_for_symbol(&engine, 'a', 260),
                observation_point_for_symbol(&engine, 'n', 300),
            ]),
        ],
    );
    let candidates = engine.feed_observation_chunk(&second);
    let top = candidates.top().unwrap();
    assert_eq!(top.text, "西安");
    assert_eq!(top.boundary.segments, ["xi", "an"]);
}

#[test]
fn candidate_preserves_joint_path() {
    let engine = ImeEngine::zh_qwerty_demo();
    let candidates = engine.decode_lattice(&exact_lattice("nihao"));
    let top = candidates.top().unwrap();
    assert_eq!(top.text, "你好");
    assert_eq!(top.reading, "ni hao");
    assert_eq!(top.schema_id.0, "zh-hans-pinyin-full");
    assert_eq!(top.keymap_id.0, "latin-qwerty");
    assert_eq!(top.symbol_path, "nihao");
    assert_eq!(top.slot_path.len(), 5);
}

#[test]
fn composed_tap_respects_chinese_profile_over_english_exact() {
    let engine = ImeEngine::zh_qwerty_with_lexicons(
        Lexicon::new([
            data_core::LexiconEntry::new("高频", "gao pin", 10_000.0),
            data_core::LexiconEntry::new("我", "wo", 1_000.0),
        ]),
        Lexicon::new([data_core::LexiconEntry::new("wo", "wo", 10_000.0)]),
        false,
    );
    let composed = engine.decode_taps_composed(&exact_points("wo"));
    let top = composed.sentences.first().unwrap();
    assert_eq!(top.total_text, "我");
    assert_eq!(top.words[0].schema_id.0, "zh-hans-pinyin-full");
}

#[test]
fn tap_lattice_penalizes_off_center_points_inside_key() {
    let engine = ImeEngine::zh_qwerty_demo();
    let keymap = LatinQwertyKeyMap::new();
    let slot_id = keymap.slot_for_symbol('w').unwrap();
    let slot = engine.geometry.slot(&slot_id).unwrap();
    let point = Point::new(
        slot.bounds.x + slot.bounds.width * 0.95,
        slot.bounds.y + slot.bounds.height * 0.5,
    );

    let lattice = engine.tap_lattice_for_points(&[point], 3);
    let best = &lattice.positions[0][0];
    assert_eq!(best.slot_id, slot_id);
    assert!(best.cost > 1.0);
}

#[test]
fn language_pack_decodes_compact_putong_as_chinese_word() {
    let engine =
        ImeEngine::zh_qwerty_from_language_packs(workspace_asset_path("assets/language-packs"))
            .unwrap();
    let candidates = engine.decode_lattice(&exact_lattice("putong"));
    let top = candidates.top().unwrap();
    assert_eq!(top.text, "普通");
    assert_eq!(top.reading, "pu tong");
}

#[test]
fn language_pack_taps_decode_putong_as_chinese_word() {
    let engine = ImeEngine::zh_qwerty_from_artifacts(
        workspace_asset_path("assets/language-packs"),
        workspace_asset_path("assets/observation-models/geometry-phone-10col/qwerty"),
    )
    .unwrap();
    let composed = engine.decode_taps_composed(&exact_points("putong"));
    let top = composed.sentences.first().unwrap();
    assert_eq!(top.total_text, "普通");
    assert_eq!(top.words[0].reading, "pu tong");
}

#[test]
fn empty_lattice_returns_no_candidates() {
    let engine = ImeEngine::zh_qwerty_demo();
    let candidates = engine.decode_lattice(&SlotLattice::default());
    assert!(candidates.candidates.is_empty());
}

#[test]
fn unknown_slots_do_not_panic() {
    let engine = ImeEngine::zh_qwerty_demo();
    let lattice = SlotLattice::new(vec![vec![SlotObservation {
        slot_id: SlotId::new("missing"),
        cost: 0.0,
    }]]);
    let candidates = engine.decode_lattice(&lattice);
    assert!(candidates.candidates.is_empty());
}

#[test]
fn can_decode_with_generated_pack_shape() {
    let engine = ImeEngine::zh_qwerty_with_lexicons(
        Lexicon::new([data_core::LexiconEntry::new("西安", "xi an", 42.0)]),
        Lexicon::new([data_core::LexiconEntry::new("layout", "layout", 10.0)]),
        false,
    );
    let candidates = engine.decode_lattice(&exact_lattice("xian"));
    assert!(
        candidates
            .candidates
            .iter()
            .any(|candidate| candidate.text == "西安" && candidate.reading == "xi an")
    );
}

#[test]
fn language_pack_context_model_feeds_contextual_continuations() {
    let pack = LanguagePack {
        root: std::path::PathBuf::new(),
        manifest: data_core::LanguagePackManifest {
            schema: "en-word".to_owned(),
            kind: None,
            source: "test".to_owned(),
            entries: 1,
            files: Vec::new(),
            components: HashMap::new(),
            format: "mocha-language-pack-v1".to_owned(),
            syllables: None,
            generated_by: None,
            generated_at_unix: None,
        },
        lexicon: Lexicon::new([]),
        schema_fst: None,
        frequency: None,
        alias_table: None,
        context_model: Some(data_core::ContextModelArtifact {
            format: "mocha-context-model-v1".to_owned(),
            schema: "en-word".to_owned(),
            unit: "word".to_owned(),
            entries: 1,
            top: vec![data_core::ContextPriorEntry {
                reading: "hello".to_owned(),
                weight: 1.0,
                prob: 1.0,
            }],
            continuations: vec![data_core::ContextContinuationEntry {
                suffix: "i".to_owned(),
                reading: "cannot".to_owned(),
                text: "cannot".to_owned(),
                weight: 100.0,
                prob: 1.0,
            }],
            use_in_energy: Some("E_context(x, r, b, z, C)".to_owned()),
            note: None,
        }),
        morphology: None,
        transliteration_table: None,
    };
    let cache = context_cache_from_language_packs(&[&pack]);
    assert_eq!(cache.score_bonus_for_schema("en-word", "hello", ""), 0.0);
    let compiled = ContextCompiler::with_base_cache(cache.clone()).compile(&TextContext {
        committed: CommittedText {
            text: "I ".to_owned(),
        },
        domain: None,
    });
    assert!(compiled.score_bonus_for_schema("en-word", "cannot", "cannot") > 0.0);
    assert_eq!(
        cache.score_bonus_for_schema("zh-hans-pinyin-full", "hello", ""),
        0.0
    );
}

#[test]
fn zh_lexicon_phrases_compile_context_continuations() {
    let lexicon = Lexicon::new([
        data_core::LexiconEntry::new("去西安", "qu xi an", 100.0),
        data_core::LexiconEntry::new("学校门口", "xue xiao men kou", 50.0),
        data_core::LexiconEntry::new("abc", "abc", 1000.0),
    ]);
    let mut cache = ContextPredictiveCache::empty();
    add_zh_lexicon_continuations(&mut cache, "zh-hans-pinyin-full", &lexicon, 10);
    assert!(cache.contextual_prediction_count() >= 3);

    let compiler = ContextCompiler::with_base_cache(cache);
    let compiled = compiler.compile(&TextContext {
        committed: CommittedText {
            text: "我想去".to_owned(),
        },
        domain: None,
    });
    assert!(compiled.score_bonus_for_schema("zh-hans-pinyin-full", "xi an", "西安") > 0.0);
    assert_eq!(
        compiled.score_bonus_for_schema("en-word", "xi an", "西安"),
        0.0
    );
}

#[test]
fn observation_model_calibrates_lattice_costs() {
    let lattice = SlotLattice::new(vec![vec![SlotObservation {
        slot_id: SlotId::new("r0c0"),
        cost: 0.2,
    }]]);
    let calibrated =
        crate::observation_model::calibrate_observation_lattice(&lattice, 0.1, &HashMap::new());
    assert!((calibrated.positions[0][0].cost - 2.0).abs() < f32::EPSILON);
}

#[test]
fn observation_model_prefers_slot_specific_units() {
    let lattice = SlotLattice::new(vec![vec![
        SlotObservation {
            slot_id: SlotId::new("r0c0"),
            cost: 0.2,
        },
        SlotObservation {
            slot_id: SlotId::new("r0c1"),
            cost: 0.2,
        },
    ]]);
    let slot_units = HashMap::from([(SlotId::new("r0c0"), 0.2)]);
    let calibrated =
        crate::observation_model::calibrate_observation_lattice(&lattice, 0.1, &slot_units);
    assert!((calibrated.positions[0][0].cost - 1.0).abs() < f32::EPSILON);
    assert!((calibrated.positions[0][1].cost - 2.0).abs() < f32::EPSILON);
}

#[test]
fn observation_pack_provides_distance_unit() {
    let pack = ObservationModelPack {
        root: std::path::PathBuf::new(),
        manifest: data_core::ObservationPackManifest {
            schema: "observation".to_owned(),
            format: "mocha-observation-pack-v1".to_owned(),
            source: "test".to_owned(),
            entries: 1,
            components: HashMap::new(),
            generated_by: None,
            generated_at_unix: None,
        },
        error_model: data_core::ObservationErrorModel {
            format: "mocha-observation-error-model-v1".to_owned(),
            geometry: "geometry-phone-10col".to_owned(),
            keymap_reference: "latin-qwerty".to_owned(),
            source: "test".to_owned(),
            samples: 1,
            features: HashMap::from([(
                "endpoint_error".to_owned(),
                data_core::FeatureStats {
                    count: 1,
                    mean: 0.125,
                    stdev: 0.0,
                    min: 0.125,
                    max: 0.125,
                },
            )]),
            slot_errors: HashMap::new(),
            use_in_energy: Some("E_obs(O, q, G)".to_owned()),
        },
        gesture_templates: None,
    };
    assert_eq!(observation_distance_unit_from_pack(&pack), Some(0.125));
}

#[test]
fn gesture_template_scoring_prefers_closest_observation_path() {
    let templates = vec![
        RuntimeGestureTemplate {
            template: "the".to_owned(),
            samples: 10,
            points: vec![
                Point::new(0.0, 0.0),
                Point::new(1.0, 0.0),
                Point::new(2.0, 0.0),
            ],
        },
        RuntimeGestureTemplate {
            template: "and".to_owned(),
            samples: 20,
            points: vec![
                Point::new(0.0, 1.0),
                Point::new(1.0, 1.0),
                Point::new(2.0, 1.0),
            ],
        },
    ];
    let trace = [
        Point::new(0.0, 0.0),
        Point::new(0.5, 0.0),
        Point::new(1.0, 0.0),
        Point::new(1.5, 0.0),
        Point::new(2.0, 0.0),
    ];
    let matches = score_gesture_templates_against(&templates, &trace, 2, 1.0);
    assert_eq!(matches[0].template, "the");
    assert!(matches[0].cost < matches[1].cost);
}

#[test]
fn gesture_trace_decodes_through_schema_candidates() {
    let engine = ImeEngine::zh_qwerty_with_lexicons(
        Lexicon::new([]),
        Lexicon::new([
            data_core::LexiconEntry::new("the", "the", 100.0),
            data_core::LexiconEntry::new("common", "common", 1000.0),
        ]),
        false,
    );
    let trace = dense_trace_through("the");
    let candidates = engine.decode_gesture_trace(&trace, 4);
    let top = candidates.top().unwrap();
    assert_eq!(top.text, "the");
    assert_eq!(top.reading, "the");
    assert_eq!(top.schema_id.0, "en-word");
    assert_eq!(top.symbol_path, "the");
    assert_eq!(top.slot_path.len(), 3);
    assert!(top.breakdown.schema > 0.0);
}

#[test]
fn gesture_decode_uses_context_when_observation_is_close() {
    let mut engine = ImeEngine::zh_qwerty_with_lexicons(
        Lexicon::new([]),
        Lexicon::new([
            data_core::LexiconEntry::new("got", "got", 100.0),
            data_core::LexiconEntry::new("hot", "hot", 100.0),
        ]),
        false,
    );
    engine.profile = KeyboardProfile::en_qwerty();
    let mut context = ContextPredictiveCache::empty();
    context.add_schema_contextual_prediction("en-word", "i", "hot", "hot", 8.0);
    engine.context_compiler = ContextCompiler::with_base_cache(context);
    engine.set_committed_context("I ");

    let trace = dense_trace_through("got");
    let candidates = engine.decode_gesture_trace(&trace, 4);
    let top = candidates.top().unwrap();
    assert_eq!(top.text, "hot");
    assert!(top.breakdown.context > 0.0);
}

#[test]
fn gesture_template_loader_rejects_empty_paths() {
    let pack = ObservationModelPack {
        root: std::path::PathBuf::new(),
        manifest: data_core::ObservationPackManifest {
            schema: "observation".to_owned(),
            format: "mocha-observation-pack-v1".to_owned(),
            source: "test".to_owned(),
            entries: 1,
            components: HashMap::new(),
            generated_by: None,
            generated_at_unix: None,
        },
        error_model: data_core::ObservationErrorModel {
            format: "mocha-observation-error-model-v1".to_owned(),
            geometry: "geometry-phone-10col".to_owned(),
            keymap_reference: "latin-qwerty".to_owned(),
            source: "test".to_owned(),
            samples: 1,
            features: HashMap::new(),
            slot_errors: HashMap::new(),
            use_in_energy: Some("E_obs(O, q, G)".to_owned()),
        },
        gesture_templates: Some(data_core::GestureTemplateArtifact {
            format: "mocha-gesture-templates-v1".to_owned(),
            geometry: "geometry-phone-10col".to_owned(),
            keymap_reference: "latin-qwerty".to_owned(),
            point_count: 2,
            templates: vec![
                data_core::GestureTemplate {
                    word: "valid".to_owned(),
                    count: 3,
                    mean_path_length: 1.0,
                    points: vec![[0.0, 0.0], [1.0, 1.0]],
                },
                data_core::GestureTemplate {
                    word: "empty".to_owned(),
                    count: 3,
                    mean_path_length: 0.0,
                    points: Vec::new(),
                },
            ],
            use_in_energy: Some("E_obs(O, q, G)".to_owned()),
        }),
    };
    let templates = gesture_templates_from_pack(&pack);
    assert_eq!(templates.len(), 1);
    assert_eq!(templates[0].template, "valid");
}

#[test]
fn en_es_profile_restores_spanish_accents() {
    let engine = ImeEngine::en_es_qwerty_demo();
    let candidates = engine.decode_lattice(&exact_spanish_lattice("manana"));
    assert!(
        candidates
            .candidates
            .iter()
            .any(|candidate| candidate.text == "mañana" && candidate.schema_id.0 == "es-word")
    );
}

#[test]
fn ru_translit_profile_generates_cyrillic() {
    let engine = ImeEngine::ru_translit_demo();
    let candidates = engine.decode_lattice(&exact_lattice("privet"));
    assert!(candidates.candidates.iter().any(|candidate| {
        candidate.text == "привет" && candidate.schema_id.0 == "ru-translit"
    }));
}

#[test]
fn ru_native_profile_uses_jcuken_slots() {
    let engine = ImeEngine::ru_native_demo();
    let candidates = engine.decode_lattice(&exact_ru_lattice("привет"));
    assert!(candidates.candidates.iter().any(|candidate| {
        candidate.text == "привет" && candidate.schema_id.0 == "ru-cyrillic"
    }));
}

#[test]
fn swipe_session_accumulates_and_clears_on_commit() {
    let mut engine = ImeEngine::zh_qwerty_demo();
    assert!(engine.swipe_session_text().is_empty());

    engine.accept_swipe_candidate("你好");
    assert_eq!(engine.swipe_session_text(), "你好");

    engine.accept_swipe_candidate("世界");
    assert_eq!(engine.swipe_session_text(), "你好世界");

    engine.reset_swipe_session();
    assert!(engine.swipe_session_text().is_empty());
    assert!(engine.swipe_lm_session.is_empty());
}

#[test]
fn accept_candidate_event_feeds_swipe_session() {
    let mut engine = ImeEngine::zh_qwerty_demo();
    let chunk = ObservationChunk::new(
        InputSource::System,
        vec![RawInputEvent::AcceptCandidate {
            text: "你好".to_owned(),
            reading: "ni hao".to_owned(),
        }],
    );
    engine.feed_observation_chunk(&chunk);
    assert_eq!(engine.swipe_session_text(), "你好");

    let commit_chunk = ObservationChunk::new(
        InputSource::System,
        vec![RawInputEvent::Commit {
            text: "你好".to_owned(),
        }],
    );
    engine.feed_observation_chunk(&commit_chunk);
    assert!(engine.swipe_session_text().is_empty());
}

#[test]
fn swipe_lm_bonus_is_zero_without_session() {
    let engine = ImeEngine::zh_qwerty_demo();
    assert_eq!(engine.swipe_lm_bonus("test"), 0.0);
}
