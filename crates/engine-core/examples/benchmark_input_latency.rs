use std::collections::HashSet;
use std::error::Error;
use std::hint::black_box;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use data_core::{LanguagePack, ObservationModelPack};
use decoder_core::CandidateList;
use engine_core::ImeEngine;
use geometry_core::{GeometryLayout, Point, SlotLattice, SlotObservation};
use geometry_phone_10col::Phone10ColGeometry;
use keymap_core::KeyMap;
use keymap_latin_qwerty::LatinQwertyKeyMap;
use lm_core::{CharacterTokenizer, LanguageModel, MiniGptConfig, MiniGptModel};
use observation_core::{InputSource, ObservationChunk, ObservationPoint, RawInputEvent};

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let zh_pack = LanguagePack::load(args.language_pack_root.join("zh-hans-pinyin-full"))?;
    let en_pack = LanguagePack::load(args.language_pack_root.join("en-word"))?;
    let observation_pack = ObservationModelPack::load(&args.observation_pack_root)?;

    println!("benchmark,category,samples,successes,ops,mean_us,p50_us,p90_us,p95_us,p99_us,max_us");
    io::stdout().flush()?;

    let zh_single = zh_cases(&zh_pack, CaseKind::SingleChar, args.zh_limit);
    let zh_words = zh_cases(&zh_pack, CaseKind::Word, args.zh_limit);
    let en_words = en_cases(&en_pack, args.en_limit);

    let mut zh_engine =
        ImeEngine::zh_qwerty_from_artifacts(&args.language_pack_root, &args.observation_pack_root)?;
    let mut en_engine =
        ImeEngine::en_qwerty_from_artifacts(&args.language_pack_root, &args.observation_pack_root)?;

    emit_summary(benchmark_slide_stream_pinyin(
        "slide_stream_pinyin",
        "zh_single",
        &zh_single,
    ));
    emit_summary(benchmark_slide_stream_pinyin(
        "slide_stream_pinyin",
        "zh_word",
        &zh_words,
    ));
    emit_summary(benchmark_slide_delayed_candidate(
        "slide_delayed_candidate",
        "zh_single",
        &zh_engine,
        &zh_single,
    ));
    emit_summary(benchmark_slide_delayed_candidate(
        "slide_delayed_candidate",
        "zh_word",
        &zh_engine,
        &zh_words,
    ));
    emit_summary(benchmark_slide_gesture(
        "slide_first_target",
        "en_word",
        &mut en_engine,
        &observation_pack,
        args.en_gesture_limit,
    ));

    emit_summary(benchmark_physical_keys(
        "physical_key_lookup",
        "zh_single",
        &zh_engine,
        &zh_single,
    ));
    emit_summary(benchmark_physical_keys(
        "physical_key_lookup",
        "zh_word",
        &zh_engine,
        &zh_words,
    ));
    emit_summary(benchmark_physical_keys(
        "physical_key_lookup",
        "en_word",
        &en_engine,
        &en_words,
    ));

    emit_summary(benchmark_autocorrect(
        "autocorrect_context_typo",
        "en_word",
        &mut en_engine,
        &en_pack,
        args.autocorrect_limit,
    ));

    // --- Noisy swipe gesture benchmarks ---
    // mild: sigma=0.01 (10% key width), curvature=0.3
    emit_summary(benchmark_swipe_noisy(
        "swipe_noisy",
        "en_mild",
        &en_engine,
        &en_words,
        0.01,
        0.3,
        1000,
    ));
    emit_summary(benchmark_swipe_noisy(
        "swipe_noisy",
        "zh_mild",
        &zh_engine,
        &zh_words,
        0.01,
        0.3,
        1000,
    ));
    // moderate: sigma=0.02 (20% key width), curvature=0.5
    emit_summary(benchmark_swipe_noisy(
        "swipe_noisy",
        "en_moderate",
        &en_engine,
        &en_words,
        0.02,
        0.5,
        1000,
    ));
    emit_summary(benchmark_swipe_noisy(
        "swipe_noisy",
        "zh_moderate",
        &zh_engine,
        &zh_words,
        0.02,
        0.5,
        1000,
    ));
    // heavy: sigma=0.03 (30% key width), curvature=0.8
    emit_summary(benchmark_swipe_noisy(
        "swipe_noisy",
        "en_heavy",
        &en_engine,
        &en_words,
        0.03,
        0.8,
        1000,
    ));
    emit_summary(benchmark_swipe_noisy(
        "swipe_noisy",
        "zh_heavy",
        &zh_engine,
        &zh_words,
        0.03,
        0.8,
        1000,
    ));

    // --- Neural LM benchmarks ---
    let lm_model = load_lm_model(&args.lm_root);
    if let Some(ref lm) = lm_model {
        let lm_ref: &dyn LanguageModel = lm;
        emit_summary(benchmark_lm_score_sequence(
            "lm_score_sequence",
            "short",
            lm_ref,
            16,
            200,
        ));
        emit_summary(benchmark_lm_score_sequence(
            "lm_score_sequence",
            "medium",
            lm_ref,
            64,
            100,
        ));
        emit_summary(benchmark_lm_score_sequence(
            "lm_score_sequence",
            "long",
            lm_ref,
            256,
            50,
        ));
        emit_summary(benchmark_lm_next_token(
            "lm_next_token",
            "short",
            lm_ref,
            16,
            200,
        ));
        emit_summary(benchmark_lm_next_token(
            "lm_next_token",
            "medium",
            lm_ref,
            64,
            100,
        ));
        emit_summary(benchmark_lm_next_token(
            "lm_next_token",
            "long",
            lm_ref,
            256,
            50,
        ));
    } else {
        eprintln!(
            "note: LM model not found at {:?}, skipping LM benchmarks",
            args.lm_root
        );
    }

    if let Some(lm) = lm_model {
        zh_engine.set_lm(Box::new(lm));
    }

    // --- Sentence composition benchmarks ---
    emit_summary(benchmark_sentence_composition(
        "sentence_composition",
        "zh_continuous",
        &zh_engine,
        &zh_words,
        200,
    ));

    // --- Swipe cross-word coherence ---
    emit_summary(benchmark_swipe_cross_word(
        "swipe_cross_word",
        "zh_word",
        &mut zh_engine,
        &zh_words,
        100,
    ));

    // Keep mutable engines observable to the optimizer across benchmark sections.
    black_box(&mut zh_engine);
    black_box(&mut en_engine);
    Ok(())
}

#[derive(Clone, Debug)]
struct Args {
    language_pack_root: PathBuf,
    observation_pack_root: PathBuf,
    lm_root: PathBuf,
    zh_limit: usize,
    en_limit: usize,
    en_gesture_limit: usize,
    autocorrect_limit: usize,
}

impl Args {
    fn parse() -> Self {
        let mut args = std::env::args().skip(1);
        Self {
            language_pack_root: args
                .next()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("assets/language-packs")),
            observation_pack_root: args.next().map(PathBuf::from).unwrap_or_else(|| {
                PathBuf::from("assets/observation-models/geometry-phone-10col/qwerty")
            }),
            lm_root: args
                .next()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("assets/lm")),
            zh_limit: args
                .next()
                .and_then(|value| value.parse().ok())
                .unwrap_or(5000),
            en_limit: args
                .next()
                .and_then(|value| value.parse().ok())
                .unwrap_or(5000),
            en_gesture_limit: args
                .next()
                .and_then(|value| value.parse().ok())
                .unwrap_or(1000),
            autocorrect_limit: args
                .next()
                .and_then(|value| value.parse().ok())
                .unwrap_or(5000),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CaseKind {
    SingleChar,
    Word,
}

#[derive(Clone, Debug)]
struct TextCase {
    text: String,
    reading: String,
    symbols: String,
}

fn zh_cases(pack: &LanguagePack, kind: CaseKind, limit: usize) -> Vec<TextCase> {
    if limit == 0 {
        return Vec::new();
    }
    let mut seen = HashSet::new();
    let mut cases = Vec::new();
    for entry in pack.lexicon.iter_entries() {
        let char_count = entry.text.chars().count();
        let is_kind = match kind {
            CaseKind::SingleChar => char_count == 1,
            CaseKind::Word => (2..=6).contains(&char_count),
        };
        if !is_kind || !entry.text.chars().all(is_cjk_unified) {
            continue;
        }
        let symbols = compact_symbols(&entry.reading);
        if !is_ascii_key_path(&symbols) || !seen.insert(entry.text.clone()) {
            continue;
        }
        cases.push(TextCase {
            text: entry.text.clone(),
            reading: entry.reading.clone(),
            symbols,
        });
        if cases.len() >= limit {
            break;
        }
    }
    cases
}

fn en_cases(pack: &LanguagePack, limit: usize) -> Vec<TextCase> {
    if limit == 0 {
        return Vec::new();
    }
    let mut seen = HashSet::new();
    let mut cases = Vec::new();
    for entry in pack.lexicon.iter_entries() {
        let symbols = compact_symbols(&entry.reading);
        if !(3..=14).contains(&symbols.len()) || !is_ascii_key_path(&symbols) {
            continue;
        }
        if !seen.insert((entry.text.clone(), entry.reading.clone())) {
            continue;
        }
        cases.push(TextCase {
            text: entry.text.clone(),
            reading: entry.reading.clone(),
            symbols,
        });
        if cases.len() >= limit {
            break;
        }
    }
    cases
}

fn benchmark_slide_stream_pinyin(
    benchmark: &'static str,
    category: &'static str,
    cases: &[TextCase],
) -> Summary {
    let mut summary = Summary::new(benchmark, category, cases.len());
    let geometry = Phone10ColGeometry::new();
    let keymap = LatinQwertyKeyMap::new();
    for case in cases {
        let Some(points) = ideal_trace_points(&case.symbols) else {
            continue;
        };
        let mut streamed = String::new();
        let mut last_slot = None;
        for point in points {
            let started = Instant::now();
            let top_slot = geometry
                .hit_test(point, 1)
                .into_iter()
                .next()
                .map(|hit| hit.slot_id);
            if top_slot.is_some() && top_slot != last_slot {
                if let Some(symbol) = top_slot
                    .as_ref()
                    .and_then(|slot| keymap.symbol_for_slot(slot, keymap_core::KeyLayer::Normal))
                {
                    streamed.push_str(&symbol.0);
                }
                last_slot = top_slot;
            }
            black_box(&streamed);
            summary.ops += 1;
            summary.observe(started.elapsed());
        }
        if streamed
            .chars()
            .next()
            .zip(case.symbols.chars().next())
            .is_some_and(|(left, right)| left == right)
        {
            summary.successes += 1;
        }
    }
    summary
}

fn benchmark_slide_delayed_candidate(
    benchmark: &'static str,
    category: &'static str,
    engine: &ImeEngine,
    cases: &[TextCase],
) -> Summary {
    let mut summary = Summary::new(benchmark, category, cases.len());
    summary.ops = cases.len();
    for case in cases {
        let Some(lattice) = exact_lattice(&case.symbols) else {
            continue;
        };
        let started = Instant::now();
        let candidates = black_box(engine.decode_lattice(&lattice));
        summary.observe(started.elapsed());
        if candidate_matches(&candidates, case) {
            summary.successes += 1;
        }
    }
    summary
}

fn benchmark_slide_gesture(
    benchmark: &'static str,
    category: &'static str,
    engine: &mut ImeEngine,
    pack: &ObservationModelPack,
    limit: usize,
) -> Summary {
    let templates = pack
        .gesture_templates
        .as_ref()
        .map(|artifact| artifact.templates.as_slice())
        .unwrap_or(&[]);
    let mut summary = Summary::new(benchmark, category, templates.len().min(limit));
    for template in templates.iter().take(limit) {
        if template.word.is_empty() || template.points.is_empty() {
            continue;
        }
        engine.reset_stream();
        let case = TextCase {
            text: template.word.clone(),
            reading: template.word.clone(),
            symbols: template.word.clone(),
        };
        let started = Instant::now();
        let mut found = false;
        for (idx, point) in template.points.iter().enumerate() {
            let chunk = ObservationChunk::new(
                InputSource::Trace,
                vec![RawInputEvent::Trace(vec![ObservationPoint::new(
                    point[0] as f32,
                    point[1] as f32,
                    idx as u64 * 16,
                )])],
            );
            let candidates = black_box(engine.feed_observation_chunk(&chunk));
            summary.ops += 1;
            if candidate_matches(&candidates, &case) {
                summary.observe(started.elapsed());
                found = true;
                break;
            }
        }
        if found {
            summary.successes += 1;
        }
    }
    engine.reset_stream();
    summary
}

fn benchmark_physical_keys(
    benchmark: &'static str,
    category: &'static str,
    engine: &ImeEngine,
    cases: &[TextCase],
) -> Summary {
    let total_ops = cases.iter().map(|case| case.symbols.len()).sum();
    let mut summary = Summary::new(benchmark, category, cases.len());
    summary.ops = total_ops;
    for case in cases {
        let mut final_candidates = CandidateList::default();
        for end in 1..=case.symbols.len() {
            let prefix = &case.symbols[..end];
            let Some(lattice) = exact_lattice(prefix) else {
                continue;
            };
            let started = Instant::now();
            final_candidates = black_box(engine.decode_lattice(&lattice));
            summary.observe(started.elapsed());
        }
        if candidate_matches(&final_candidates, case) {
            summary.successes += 1;
        }
    }
    summary
}

fn benchmark_autocorrect(
    benchmark: &'static str,
    category: &'static str,
    engine: &mut ImeEngine,
    pack: &LanguagePack,
    limit: usize,
) -> Summary {
    if limit == 0 {
        return Summary::new(benchmark, category, 0);
    }
    let continuations = pack
        .context_model
        .as_ref()
        .map(|model| model.continuations.as_slice())
        .unwrap_or(&[]);
    let mut cases = Vec::new();
    let mut seen = HashSet::new();
    for continuation in continuations {
        let symbols = compact_symbols(&continuation.reading);
        if !(4..=14).contains(&symbols.len()) || !is_ascii_key_path(&symbols) {
            continue;
        }
        let Some(typo) = typo_for(&symbols) else {
            continue;
        };
        if !seen.insert((
            continuation.suffix.clone(),
            continuation.text.clone(),
            typo.clone(),
        )) {
            continue;
        }
        cases.push(AutocorrectCase {
            previous: continuation.suffix.clone(),
            target: TextCase {
                text: continuation.text.clone(),
                reading: continuation.reading.clone(),
                symbols,
            },
            typo,
        });
        if cases.len() >= limit {
            break;
        }
    }

    let mut summary = Summary::new(benchmark, category, cases.len());
    for case in &cases {
        let Some(lattice) = exact_lattice(&case.typo) else {
            continue;
        };
        let started = Instant::now();
        engine.set_committed_context(&case.previous);
        let candidates = black_box(engine.decode_lattice(&lattice));
        summary.ops += 1;
        summary.observe(started.elapsed());
        if candidate_matches(&candidates, &case.target) {
            summary.successes += 1;
        }
    }
    engine.set_committed_context("");
    summary
}

#[derive(Clone, Debug)]
struct AutocorrectCase {
    previous: String,
    target: TextCase,
    typo: String,
}

#[derive(Clone, Debug)]
struct Summary {
    benchmark: &'static str,
    category: &'static str,
    samples: usize,
    successes: usize,
    ops: usize,
    latencies: Vec<Duration>,
}

impl Summary {
    fn new(benchmark: &'static str, category: &'static str, samples: usize) -> Self {
        Self {
            benchmark,
            category,
            samples,
            successes: 0,
            ops: 0,
            latencies: Vec::new(),
        }
    }

    fn observe(&mut self, duration: Duration) {
        self.latencies.push(duration);
    }
}

fn emit_summary(mut summary: Summary) {
    summary.latencies.sort();
    let mean = if summary.latencies.is_empty() {
        0.0
    } else {
        summary
            .latencies
            .iter()
            .map(|duration| duration.as_secs_f64() * 1_000_000.0)
            .sum::<f64>()
            / summary.latencies.len() as f64
    };
    println!(
        "{},{},{},{},{},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2}",
        summary.benchmark,
        summary.category,
        summary.samples,
        summary.successes,
        summary.ops,
        mean,
        percentile_us(&summary.latencies, 0.50),
        percentile_us(&summary.latencies, 0.90),
        percentile_us(&summary.latencies, 0.95),
        percentile_us(&summary.latencies, 0.99),
        percentile_us(&summary.latencies, 1.00),
    );
    io::stdout().flush().expect("flush benchmark row");
}

fn percentile_us(values: &[Duration], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let idx = ((values.len() - 1) as f64 * p).round() as usize;
    values[idx].as_secs_f64() * 1_000_000.0
}

fn candidate_matches(candidates: &CandidateList, case: &TextCase) -> bool {
    candidates
        .candidates
        .iter()
        .take(5)
        .any(|candidate| candidate.text == case.text || candidate.reading == case.reading)
}

fn ideal_trace_points(symbols: &str) -> Option<Vec<Point>> {
    let geometry = Phone10ColGeometry::new();
    let keymap = LatinQwertyKeyMap::new();
    let centers = symbols
        .chars()
        .map(|ch| {
            keymap
                .slot_for_symbol(ch)
                .and_then(|slot| geometry.slot(&slot).map(|slot| slot.center()))
        })
        .collect::<Option<Vec<_>>>()?;
    if centers.is_empty() {
        return None;
    }
    let mut points = Vec::new();
    points.push(centers[0]);
    for pair in centers.windows(2) {
        let start = pair[0];
        let end = pair[1];
        for step in 1..=4 {
            let t = step as f32 / 4.0;
            points.push(Point::new(
                start.x + (end.x - start.x) * t,
                start.y + (end.y - start.y) * t,
            ));
        }
    }
    Some(points)
}

fn exact_lattice(symbols: &str) -> Option<SlotLattice> {
    let keymap = LatinQwertyKeyMap::new();
    symbols
        .chars()
        .map(|ch| {
            keymap
                .slot_for_symbol(ch)
                .map(|slot_id| vec![SlotObservation { slot_id, cost: 0.0 }])
        })
        .collect::<Option<Vec<_>>>()
        .map(SlotLattice::new)
}

fn typo_for(symbols: &str) -> Option<String> {
    let mut chars = symbols.chars().collect::<Vec<_>>();
    if chars.len() < 4 {
        return None;
    }
    let idx = (chars.len() / 2).saturating_sub(1);
    chars.swap(idx, idx + 1);
    let typo = chars.into_iter().collect::<String>();
    (typo != symbols).then_some(typo)
}

fn compact_symbols(reading: &str) -> String {
    reading
        .chars()
        .filter(|ch| ch.is_ascii_alphabetic())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

fn is_ascii_key_path(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|ch| ch.is_ascii_lowercase())
}

fn is_cjk_unified(ch: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&ch)
}

fn load_lm_model(lm_root: &PathBuf) -> Option<MiniGptModel> {
    let model_path = lm_root.join("model.safetensors");
    let config_path = lm_root.join("config.json");
    let tokenizer_path = lm_root.join("tokenizer.json");

    if !model_path.exists() {
        return None;
    }

    let config: MiniGptConfig = if config_path.exists() {
        let data = std::fs::read_to_string(&config_path).ok()?;
        serde_json::from_str(&data).ok()?
    } else {
        MiniGptConfig::default()
    };

    let tokenizer = if tokenizer_path.exists() {
        CharacterTokenizer::from_json_path(&tokenizer_path).ok()?
    } else {
        CharacterTokenizer::builtin()
    };

    let device = candle_core::Device::Cpu;
    MiniGptModel::load(&model_path, config, tokenizer, &device).ok()
}

fn benchmark_lm_score_sequence(
    benchmark: &'static str,
    category: &'static str,
    lm: &dyn LanguageModel,
    seq_len: usize,
    iterations: usize,
) -> Summary {
    let mut summary = Summary::new(benchmark, category, iterations);
    summary.ops = iterations;
    let dummy_ids: Vec<u32> = (3..3 + seq_len as u32).collect();
    for _ in 0..iterations {
        let started = Instant::now();
        let score = black_box(lm.score_sequence(&dummy_ids));
        summary.observe(started.elapsed());
        black_box(score);
        summary.successes += 1;
    }
    summary
}

fn benchmark_lm_next_token(
    benchmark: &'static str,
    category: &'static str,
    lm: &dyn LanguageModel,
    seq_len: usize,
    iterations: usize,
) -> Summary {
    let mut summary = Summary::new(benchmark, category, iterations);
    summary.ops = iterations;
    let dummy_ids: Vec<u32> = (3..3 + seq_len as u32).collect();
    for _ in 0..iterations {
        let started = Instant::now();
        let logprobs = black_box(lm.next_token_logprobs(&dummy_ids));
        summary.observe(started.elapsed());
        black_box(&logprobs);
        summary.successes += 1;
    }
    summary
}

fn benchmark_sentence_composition(
    benchmark: &'static str,
    category: &'static str,
    engine: &ImeEngine,
    cases: &[TextCase],
    limit: usize,
) -> Summary {
    let count = cases.len().min(limit);
    let mut summary = Summary::new(benchmark, category, count);
    summary.ops = count;
    for case in cases.iter().take(limit) {
        if case.symbols.len() < 4 {
            continue;
        }
        let Some(points) = ideal_tap_points(&case.symbols) else {
            continue;
        };
        let started = Instant::now();
        let result = black_box(engine.decode_taps_composed(&points));
        summary.observe(started.elapsed());
        if !result.sentences.is_empty()
            || result
                .word_candidates
                .candidates
                .iter()
                .any(|c| c.text == case.text)
        {
            summary.successes += 1;
        }
    }
    summary
}

fn benchmark_swipe_cross_word(
    benchmark: &'static str,
    category: &'static str,
    engine: &mut ImeEngine,
    cases: &[TextCase],
    limit: usize,
) -> Summary {
    let count = cases.len().min(limit);
    let mut summary = Summary::new(benchmark, category, count);
    engine.reset_swipe_session();

    for (i, case) in cases.iter().take(limit).enumerate() {
        let started = Instant::now();
        engine.accept_swipe_candidate(&case.text);
        summary.observe(started.elapsed());
        summary.ops += 1;

        if (i + 1) % 5 == 0 {
            engine.reset_swipe_session();
        }
    }

    summary.successes = count;
    engine.reset_swipe_session();
    summary
}

fn ideal_tap_points(symbols: &str) -> Option<Vec<Point>> {
    let geometry = Phone10ColGeometry::new();
    let keymap = LatinQwertyKeyMap::new();
    symbols
        .chars()
        .map(|ch| {
            keymap
                .slot_for_symbol(ch)
                .and_then(|slot| geometry.slot(&slot).map(|s| s.center()))
        })
        .collect::<Option<Vec<_>>>()
}

struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(1),
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn next_gaussian(&mut self) -> f32 {
        let u1 = self.next_f32().max(1e-10);
        let u2 = self.next_f32();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos()
    }
}

fn noisy_swipe_trace(
    symbols: &str,
    noise_sigma: f32,
    curvature: f32,
    seed: u64,
) -> Option<Vec<Point>> {
    let geometry = Phone10ColGeometry::new();
    let keymap = LatinQwertyKeyMap::new();
    let mut rng = SimpleRng::new(seed);

    let centers: Vec<Point> = symbols
        .chars()
        .map(|ch| {
            keymap
                .slot_for_symbol(ch)
                .and_then(|slot| geometry.slot(&slot).map(|s| s.center()))
        })
        .collect::<Option<Vec<_>>>()?;

    if centers.len() < 2 {
        return None;
    }

    let targets: Vec<Point> = centers
        .iter()
        .map(|c| {
            Point::new(
                c.x + rng.next_gaussian() * noise_sigma * 0.5,
                c.y + rng.next_gaussian() * noise_sigma * 0.5,
            )
        })
        .collect();

    let points_per_segment = 8;
    let mut points = Vec::with_capacity(targets.len() * points_per_segment);

    points.push(Point::new(
        targets[0].x + rng.next_gaussian() * noise_sigma,
        targets[0].y + rng.next_gaussian() * noise_sigma,
    ));

    for pair in targets.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let mid_x = (start.x + end.x) * 0.5;
        let mid_y = (start.y + end.y) * 0.5;
        let bend = (rng.next_f32() * 2.0 - 1.0) * curvature;
        let ctrl_x = mid_x + (-dy) * bend;
        let ctrl_y = mid_y + dx * bend;

        for step in 1..=points_per_segment {
            let t = step as f32 / points_per_segment as f32;
            let inv = 1.0 - t;
            let bx = inv * inv * start.x + 2.0 * inv * t * ctrl_x + t * t * end.x;
            let by = inv * inv * start.y + 2.0 * inv * t * ctrl_y + t * t * end.y;
            points.push(Point::new(
                bx + rng.next_gaussian() * noise_sigma,
                by + rng.next_gaussian() * noise_sigma,
            ));
        }
    }

    Some(points)
}

fn benchmark_swipe_noisy(
    benchmark: &'static str,
    category: &'static str,
    engine: &ImeEngine,
    cases: &[TextCase],
    noise_sigma: f32,
    curvature: f32,
    limit: usize,
) -> Summary {
    let count = cases.len().min(limit);
    let mut summary = Summary::new(benchmark, category, count);
    for (i, case) in cases.iter().take(limit).enumerate() {
        if case.symbols.len() < 3 {
            continue;
        }
        let Some(trace) =
            noisy_swipe_trace(&case.symbols, noise_sigma, curvature, i as u64 * 31337)
        else {
            continue;
        };
        let started = Instant::now();
        let candidates = black_box(engine.decode_gesture_trace(&trace, 10));
        summary.observe(started.elapsed());
        summary.ops += 1;
        if candidate_matches(&candidates, &case) {
            summary.successes += 1;
        }
    }
    summary
}
