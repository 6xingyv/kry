//! Measures how much the trained MiniGPT context LM improves swipe decoding when
//! preceding words provide context. Each sentence is decoded word-by-word; the
//! correct prior words are fed into the swipe LM session (teacher forcing), so the
//! only difference between the two runs is whether the LM is loaded.
//!
//! Usage: cargo run --release -p engine-core --example validate_lm_context

use std::error::Error;
use std::path::PathBuf;

use engine_core::ImeEngine;
use geometry_core::{GeometryLayout, Point};
use geometry_phone_10col::Phone10ColGeometry;
use keymap_latin_qwerty::LatinQwertyKeyMap;

fn main() -> Result<(), Box<dyn Error>> {
    let pack_root = PathBuf::from("assets/language-packs");
    let obs_root = PathBuf::from("assets/observation-models/geometry-phone-10col/qwerty");
    let lm_dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "assets/lm".to_owned());
    let geometry = Phone10ColGeometry::new();
    let keymap = LatinQwertyKeyMap::new();
    let article = article();
    let total: usize = article.iter().map(|s| s.len()).sum();

    // Noise makes the geometric decode fail on some words (at no_noise it is ~100%
    // and context has nothing to fix); the LM's job is to recover those via context.
    let noise: f32 = std::env::args()
        .nth(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.045);

    // ── baseline: NullLanguageModel (context fed but ignored) ──
    let mut engine = ImeEngine::zh_qwerty_from_artifacts(&pack_root, &obs_root)?;
    let (base_top1, base_flips) = run(&mut engine, &geometry, &keymap, &article, noise);

    // ── with trained LM ──
    engine.load_lm_from_dir(&lm_dir)?;
    let (lm_top1, lm_flips) = run(&mut engine, &geometry, &keymap, &article, noise);

    println!("words={total}  noise={noise}");
    println!(
        "baseline (no LM)  reading_top1 = {:.1}%  ({}/{})",
        base_top1 as f64 / total as f64 * 100.0,
        base_top1,
        total
    );
    println!(
        "with trained LM   reading_top1 = {:.1}%  ({}/{})",
        lm_top1 as f64 / total as f64 * 100.0,
        lm_top1,
        total
    );

    // Words the LM fixed (wrong→right) and broke (right→wrong)
    let fixed: Vec<_> = lm_flips
        .iter()
        .filter(|(w, _, ok)| *ok && base_flips.iter().any(|(b, _, bok)| b == w && !bok))
        .collect();
    let broke: Vec<_> = lm_flips
        .iter()
        .filter(|(w, _, ok)| !*ok && base_flips.iter().any(|(b, _, bok)| b == w && *bok))
        .collect();
    println!("\nLM fixed ({}):", fixed.len());
    for (w, got, _) in &fixed {
        println!("  ✓ {w}  (baseline got {got})");
    }
    if !broke.is_empty() {
        println!("\nLM broke ({}):", broke.len());
        for (w, got, _) in &broke {
            println!("  ✗ {w}  (LM got {got})");
        }
    }
    Ok(())
}

/// Decodes every word with the correct prior words as LM context. Returns
/// (#top1 reading correct, per-word (text, got_text, correct?) records).
fn run(
    engine: &mut ImeEngine,
    geometry: &Phone10ColGeometry,
    keymap: &LatinQwertyKeyMap,
    article: &[Vec<(&str, &str)>],
    noise: f32,
) -> (usize, Vec<(String, String, bool)>) {
    let mut top1 = 0;
    let mut records = Vec::new();
    let mut seed = 0u64;
    for sentence in article {
        engine.reset_swipe_session();
        for &(text, reading) in sentence {
            seed += 1;
            let symbols: String = reading.chars().filter(|c| *c != ' ').collect();
            let Some(trace) = dense_trace(&symbols, geometry, keymap, noise, seed) else {
                continue;
            };
            let candidates = engine.decode_gesture_trace(&trace, 8);
            let got = candidates.top();
            let ok = got.is_some_and(|c| c.reading == reading);
            if ok {
                top1 += 1;
            }
            let got_text = got
                .map(|c| c.text.clone())
                .unwrap_or_else(|| "<none>".into());
            // Diagnostic: when wrong, show the context logprob gap correct−got.
            if !ok && !engine.swipe_session_text().is_empty() {
                let lp_correct = engine.swipe_lm_logprob(text);
                let lp_got = engine.swipe_lm_logprob(&got_text);
                println!(
                    "    ctx[{}] want {text}(logP {lp_correct:.2}) got {got_text}(logP {lp_got:.2}) gap {:.2}",
                    engine.swipe_session_text(),
                    lp_correct - lp_got,
                );
            }
            records.push((text.to_owned(), got_text, ok));
            // teacher forcing: feed the correct word as context for the next word
            engine.accept_swipe_candidate(text);
        }
    }
    (top1, records)
}

fn dense_trace(
    symbols: &str,
    geometry: &Phone10ColGeometry,
    keymap: &LatinQwertyKeyMap,
    noise: f32,
    seed: u64,
) -> Option<Vec<Point>> {
    let centers: Vec<Point> = symbols
        .chars()
        .filter_map(|ch| Some(geometry.slot(&keymap.slot_for_symbol(ch)?)?.center()))
        .collect();
    if centers.len() < 2 {
        return None;
    }
    let pps = 8;
    let mut trace = vec![centers[0]];
    for i in 1..centers.len() {
        let (from, to) = (centers[i - 1], centers[i]);
        for j in 1..=pps {
            let t = j as f32 / pps as f32;
            trace.push(Point::new(
                from.x + (to.x - from.x) * t,
                from.y + (to.y - from.y) * t,
            ));
        }
    }
    if noise > 0.0 {
        let mut state = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        for p in &mut trace {
            for axis in 0..2 {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let d = ((state >> 33) as f32 / u32::MAX as f32 - 0.5) * 2.0 * noise;
                if axis == 0 {
                    p.x += d;
                } else {
                    p.y += d;
                }
            }
        }
    }
    Some(trace)
}

/// Encyclopedic (Wikipedia-domain) sentences so the corpus training transfers,
/// segmented into (text, reading) words; readings match the lexicon (ü→u). Mixes
/// strong collocations with words that isolated geometry/frequency mis-ranks, so
/// preceding context has a chance to correct them.
fn article() -> Vec<Vec<(&'static str, &'static str)>> {
    vec![
        vec![
            ("随着", "sui zhe"),
            ("移动", "yi dong"),
            ("互联网", "hu lian wang"),
            ("的", "de"),
            ("快速", "kuai su"),
            ("发展", "fa zhan"),
        ],
        vec![
            ("人工", "ren gong"),
            ("智能", "zhi neng"),
            ("技术", "ji shu"),
            ("得到", "de dao"),
            ("广泛", "guang fan"),
            ("应用", "ying yong"),
        ],
        vec![
            ("经济", "jing ji"),
            ("增长", "zeng zhang"),
            ("速度", "su du"),
            ("明显", "ming xian"),
            ("放缓", "fang huan"),
        ],
        vec![
            ("价格", "jia ge"),
            ("指数", "zhi shu"),
            ("持续", "chi xu"),
            ("上涨", "shang zhang"),
        ],
        vec![
            ("政府", "zheng fu"),
            ("当时", "dang shi"),
            ("采取", "cai qu"),
            ("有效", "you xiao"),
            ("措施", "cuo shi"),
        ],
        vec![
            ("这", "zhe"),
            ("项", "xiang"),
            ("研究", "yan jiu"),
            ("灵活", "ling huo"),
            ("运用", "yun yong"),
            ("数学", "shu xue"),
            ("模型", "mo xing"),
        ],
        vec![
            ("中文", "zhong wen"),
            ("输入", "shu ru"),
            ("面临", "mian lin"),
            ("独特", "du te"),
            ("挑战", "tiao zhan"),
        ],
        vec![
            ("科学家", "ke xue jia"),
            ("提出", "ti chu"),
            ("全新", "quan xin"),
            ("理论", "li lun"),
        ],
    ]
}
