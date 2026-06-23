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
    let engine = ImeEngine::zh_qwerty_from_artifacts(&pack_root, &obs_root)?;
    let g = Phone10ColGeometry::new();
    let k = LatinQwertyKeyMap::new();

    // (pinyin swipe path, expected word)
    let cases = [
        ("women", "我们"),
        ("shijian", "时间"),
        ("zhongguo", "中国"),
        ("beijing", "北京"),
        ("xuexiao", "学校"),
        ("pengyou", "朋友"),
        ("woaini", "我爱你"),
        ("xiexie", "谢谢"),
        ("zhidao", "知道"),
        ("yinwei", "因为"),
        // English-in-Chinese routing: zh can't parse these, router should surface en.
        ("the", "the"),
        ("keyboard", "keyboard"),
    ];
    for (py, want) in cases {
        let trace = ideal_trace(py, &g, &k);
        let cands = engine.decode_gesture_trace(&trace, 8);
        let top = cands.candidates.first();
        let ok = top.map(|c| c.text == want).unwrap_or(false);
        println!("== {py} (want {want}) {} ==", if ok { "OK" } else { "MISS" });
        for c in cands.candidates.iter().take(4) {
            let b = &c.breakdown;
            println!(
                "   {:<8} ({:<10}) total={:.3} obs={:.3} schema={:.3}",
                c.text, c.reading, c.score, b.observation, b.schema
            );
        }
    }
    Ok(())
}
