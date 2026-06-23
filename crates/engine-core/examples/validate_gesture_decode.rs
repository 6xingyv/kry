use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;

use data_core::{LanguagePack, ObservationModelPack};
use engine_core::ImeEngine;
use geometry_core::{GeometryLayout, Point};
use geometry_phone_10col::Phone10ColGeometry;
use keymap_latin_qwerty::LatinQwertyKeyMap;

fn main() -> Result<(), Box<dyn Error>> {
    let pack_root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("assets/language-packs"));
    let observation_root = std::env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("assets/observation-models/geometry-phone-10col/qwerty"));
    let limit = std::env::args()
        .nth(3)
        .and_then(|value| value.parse::<usize>().ok());

    let engine = ImeEngine::en_qwerty_from_artifacts(&pack_root, &observation_root)?;
    let pack = ObservationModelPack::load(&observation_root)?;

    // Per-word frequency, so the benchmark can be judged the way users feel it:
    // a miss on "it"/"the" counts far more than a miss on a rare word.
    let en_lexicon = LanguagePack::load(pack_root.join("en-word"))?.lexicon;
    let mut weights: HashMap<String, f64> = HashMap::new();
    for e in en_lexicon.iter_entries() {
        let w = weights.entry(e.reading.clone()).or_insert(0.0);
        *w = (*w).max(e.weight as f64);
    }
    let weight_of = |word: &str| weights.get(word).copied().unwrap_or(1.0).max(1.0);

    let words: Vec<String> = pack
        .gesture_templates
        .as_ref()
        .map(|artifact| {
            artifact
                .templates
                .iter()
                .filter(|t| t.word.len() >= 2 && t.word.chars().all(|c| c.is_ascii_lowercase()))
                .map(|t| t.word.clone())
                .collect()
        })
        .unwrap_or_default();

    let geometry = Phone10ColGeometry::new();
    let keymap = LatinQwertyKeyMap::new();

    let noise_levels: &[(&str, f32)] = &[
        ("no_noise", 0.0),
        ("mild", 0.015),
        ("moderate", 0.03),
        ("heavy", 0.05),
    ];

    for &(label, noise_scale) in noise_levels {
        let mut total = 0usize;
        let mut top1 = 0usize;
        let mut top5 = 0usize;
        let mut no_candidate = 0usize;
        // Frequency-weighted accumulators: Σ(freq·correct) / Σ(freq).
        let mut w_total = 0.0f64;
        let mut w_top1 = 0.0f64;
        let mut w_top5 = 0.0f64;
        let mut mismatches = Vec::new();

        for (idx, word) in words.iter().take(limit.unwrap_or(usize::MAX)).enumerate() {
            let Some(trace) = generate_trace(word, &geometry, &keymap, noise_scale, idx as u64)
            else {
                continue;
            };
            let candidates = engine.decode_gesture_trace(&trace, 8);
            total += 1;
            let weight = weight_of(word);
            w_total += weight;

            let top = candidates.top();
            if top.is_some_and(|c| candidate_matches(c, word)) {
                top1 += 1;
                w_top1 += weight;
            }
            if candidates
                .candidates
                .iter()
                .take(5)
                .any(|c| candidate_matches(c, word))
            {
                top5 += 1;
                w_top5 += weight;
            }
            if candidates.candidates.is_empty() {
                no_candidate += 1;
            } else if !top.is_some_and(|c| candidate_matches(c, word)) {
                let top_c = top.unwrap();
                mismatches.push((weight, word.clone(), top_c.text.clone()));
            }
        }

        if total > 0 {
            println!(
                "{label:>10} n={total} | freq-weighted top1={:.1}% top5={:.1}% | unweighted top1={:.1}% top5={:.1}% no_cand={no_candidate}",
                w_top1 / w_total * 100.0,
                w_top5 / w_total * 100.0,
                top1 as f64 / total as f64 * 100.0,
                top5 as f64 / total as f64 * 100.0,
            );
        }
        if label == "mild" {
            // Highest-frequency failures first — these are what users actually hit.
            mismatches.sort_by(|a, b| b.0.total_cmp(&a.0));
            for (weight, expected, got_text) in mismatches.iter().take(15) {
                println!("  high-freq miss: {expected} -> {got_text}  (freq={weight:.0})");
            }
        }
    }

    Ok(())
}

fn candidate_matches(candidate: &decoder_core::Candidate, expected: &str) -> bool {
    candidate.text == expected || candidate.reading == expected
}

fn generate_trace(
    word: &str,
    geometry: &Phone10ColGeometry,
    keymap: &LatinQwertyKeyMap,
    noise_scale: f32,
    seed: u64,
) -> Option<Vec<Point>> {
    let centers: Vec<Point> = word
        .chars()
        .map(|ch| {
            let slot_id = keymap.slot_for_symbol(ch)?;
            Some(geometry.slot(&slot_id)?.center())
        })
        .collect::<Option<Vec<_>>>()?;

    if centers.len() < 2 {
        return None;
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

    if noise_scale > 0.0 {
        let mut state = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        for point in &mut trace {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let nx = ((state >> 33) as f32 / u32::MAX as f32 - 0.5) * 2.0 * noise_scale;
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let ny = ((state >> 33) as f32 / u32::MAX as f32 - 0.5) * 2.0 * noise_scale;
            point.x += nx;
            point.y += ny;
        }
    }

    Some(trace)
}
