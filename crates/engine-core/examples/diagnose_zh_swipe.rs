use std::error::Error;
use std::path::PathBuf;

use engine_core::{ImeEngine, debug_path_costs};
use geometry_core::{GeometryLayout, Point};
use geometry_phone_10col::Phone10ColGeometry;
use keymap_latin_qwerty::LatinQwertyKeyMap;

fn main() -> Result<(), Box<dyn Error>> {
    let pack_root = PathBuf::from("assets/language-packs");
    let observation_root = PathBuf::from("assets/observation-models/geometry-phone-10col/qwerty");
    let engine = ImeEngine::zh_qwerty_from_artifacts(&pack_root, &observation_root)?;
    let geometry = Phone10ColGeometry::new();
    let keymap = LatinQwertyKeyMap::new();

    // (期望文字, 期望读音, 符号路径, 近音错误路径)
    let cases: &[(&str, &str, &str, &str)] = &[
        ("当时", "dang shi", "dangshi", "danshi"),
        ("我是", "wo shi", "woshi", "qishi"),
        ("是个", "shi ge", "shige", "shifei"),
        ("移动", "yi dong", "yidong", "yiding"),
        ("面临", "mian lin", "mianlin", "miankong"),
    ];

    // 观测单位 = mean_key_width * 0.5 = 0.1 * 0.5（Phone10Col）
    let unit = 0.05f32;
    println!("── 三度量分量对比（正确路径 vs 近音错误路径，单位已除以 {unit}）──");
    for &(text, _reading, correct, wrong) in cases {
        let tr = generate_trace(correct, &geometry, &keymap).unwrap();
        let kc = key_centers(correct, &geometry, &keymap);
        let kw = key_centers(wrong, &geometry, &keymap);
        let (c1, c2, cd) = debug_path_costs(&tr, &kc, unit);
        let (w1, w2, wd) = debug_path_costs(&tr, &kw, unit);
        println!(
            "  {text}: 正确[{correct}] v1={c1:.3} v2={c2:.3} dtw={cd:.3} min={:.3}  |  错误[{wrong}] v1={w1:.3} v2={w2:.3} dtw={wd:.3} min={:.3}",
            c1.min(c2).min(cd),
            w1.min(w2).min(wd),
        );
    }
    println!();

    for &(text, reading, path, _wrong) in cases {
        let Some(trace) = generate_trace(path, &geometry, &keymap) else {
            println!("!! 无法生成轨迹 {path}");
            continue;
        };
        let candidates = engine.decode_gesture_trace(&trace, 8);
        println!(
            "\n══ 期望 {text}({reading})  路径={path}  轨迹点数={} ══",
            trace.len()
        );
        for (i, c) in candidates.candidates.iter().take(8).enumerate() {
            let mark = if c.reading == reading {
                " ✓读音"
            } else {
                ""
            };
            let b = &c.breakdown;
            println!(
                "  #{i} {:>6}({:<12}) total={:.3}  obs={:.3} sch={:.3} prof={:.2} ctx={:.2}{mark}",
                c.text, c.reading, c.score, b.observation, b.schema, b.profile, b.context,
            );
        }
        // 正确读音若不在前 8，全量扫描看排名
        let rank = candidates
            .candidates
            .iter()
            .position(|c| c.reading == reading);
        match rank {
            Some(r) if r >= 8 => println!(
                "  >> 正确读音排名 #{r}（候选总数 {}）",
                candidates.candidates.len()
            ),
            None => println!(
                "  >> 正确读音完全不在候选中（共 {} 个）",
                candidates.candidates.len()
            ),
            _ => {}
        }
    }

    Ok(())
}

fn key_centers(
    symbol_path: &str,
    geometry: &Phone10ColGeometry,
    keymap: &LatinQwertyKeyMap,
) -> Vec<Point> {
    symbol_path
        .chars()
        .filter_map(|ch| {
            let slot_id = keymap.slot_for_symbol(ch)?;
            Some(geometry.slot(&slot_id)?.center())
        })
        .collect()
}

fn generate_trace(
    symbol_path: &str,
    geometry: &Phone10ColGeometry,
    keymap: &LatinQwertyKeyMap,
) -> Option<Vec<Point>> {
    let centers = key_centers(symbol_path, geometry, keymap);
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
    Some(trace)
}
