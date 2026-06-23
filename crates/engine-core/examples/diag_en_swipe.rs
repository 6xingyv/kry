use std::error::Error;
use std::path::PathBuf;

use engine_core::ImeEngine;
use geometry_core::{GeometryLayout, Point};
use geometry_phone_10col::Phone10ColGeometry;
use keymap_latin_qwerty::LatinQwertyKeyMap;

fn ideal_trace(word: &str, g: &Phone10ColGeometry, k: &LatinQwertyKeyMap) -> Vec<Point> {
    let centers: Vec<Point> = word
        .chars()
        .map(|c| g.slot(&k.slot_for_symbol(c).unwrap()).unwrap().center())
        .collect();
    let mut trace = vec![centers[0]];
    for i in 1..centers.len() {
        let (from, to) = (centers[i - 1], centers[i]);
        for j in 1..=8 {
            let t = j as f32 / 8.0;
            trace.push(Point::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t));
        }
    }
    trace
}

fn main() -> Result<(), Box<dyn Error>> {
    let pack_root = PathBuf::from("assets/language-packs");
    let obs_root = PathBuf::from("assets/observation-models/geometry-phone-10col/qwerty");
    let engine = ImeEngine::en_qwerty_from_language_packs(&pack_root)
        .or_else(|_| ImeEngine::zh_qwerty_from_artifacts(&pack_root, &obs_root))?;
    let g = Phone10ColGeometry::new();
    let k = LatinQwertyKeyMap::new();

    for word in ["it", "or", "we", "to", "four", "is", "the", "international"] {
        let trace = ideal_trace(word, &g, &k);
        let cands = engine.decode_gesture_trace(&trace, 8);
        println!("== {word} (want '{word}') ==");
        for c in cands.candidates.iter().take(5) {
            let b = &c.breakdown;
            println!(
                "   {:<14} total={:.3} obs={:.3} schema={:.3} profile={:.3}",
                c.text, c.score, b.observation, b.schema, b.profile
            );
        }
    }
    Ok(())
}
