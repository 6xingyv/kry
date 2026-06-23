// Decode a swipe given the raw KEY PATH the finger crosses (not just the target
// letters). Usage: cargo run -p engine-core --example diag_swipe_path -- njihgfdsasdfguio
use std::error::Error;
use std::path::PathBuf;

use engine_core::ImeEngine;
use geometry_core::{GeometryLayout, Point};
use geometry_phone_10col::Phone10ColGeometry;
use keymap_latin_qwerty::LatinQwertyKeyMap;

fn path_trace(keys: &str, g: &Phone10ColGeometry, k: &LatinQwertyKeyMap) -> Vec<Point> {
    let centers: Vec<Point> = keys
        .chars()
        .filter_map(|c| k.slot_for_symbol(c).and_then(|s| g.slot(&s)).map(|s| s.center()))
        .collect();
    let mut trace = vec![centers[0]];
    for i in 1..centers.len() {
        let (a, b) = (centers[i - 1], centers[i]);
        for j in 1..=8 {
            let t = j as f32 / 8.0;
            trace.push(Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
        }
    }
    trace
}

fn main() -> Result<(), Box<dyn Error>> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "njihgfdsasdfguio".to_owned());
    let pack = PathBuf::from("assets/language-packs");
    let obs = PathBuf::from("assets/observation-models/geometry-phone-10col/qwerty");
    let engine = ImeEngine::zh_qwerty_from_artifacts(&pack, &obs)?;
    let g = Phone10ColGeometry::new();
    let k = LatinQwertyKeyMap::new();

    let trace = path_trace(&path, &g, &k);
    println!("key path: {path}  ({} pts)", trace.len());
    let cands = engine.decode_gesture_trace(&trace, 12);
    for (i, c) in cands.candidates.iter().take(12).enumerate() {
        let b = &c.breakdown;
        println!(
            "  {i:>2}. {:<10} ({:<10}) total={:.2} obs={:.2} schema={:.2}",
            c.text, c.reading, c.score, b.observation, b.schema
        );
    }
    let hit = cands.candidates.iter().position(|c| c.text == "你好");
    println!("你好 rank: {:?}", hit);
    Ok(())
}
