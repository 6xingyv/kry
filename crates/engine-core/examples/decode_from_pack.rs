use std::error::Error;
use std::path::PathBuf;

use data_core::ObservationModelPack;
use engine_core::ImeEngine;
use geometry_core::{Point, SlotLattice, SlotObservation};
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
    let mut engine = ImeEngine::zh_qwerty_from_artifacts(&pack_root, &observation_root)?;

    for input in [
        "x",
        "xian",
        "xianm",
        "yige",
        "layout",
        "teh",
        "keyboarf",
        "copyright",
    ] {
        let candidates = engine.decode_lattice(&exact_lattice(input));
        println!("input={input}");
        for candidate in candidates.candidates.iter().take(5) {
            println!(
                "  text={} reading={} schema={} score={:.3}",
                candidate.text, candidate.reading, candidate.schema_id.0, candidate.score
            );
        }
    }

    engine.set_committed_context("我想去");
    let candidates = engine.decode_lattice(&exact_lattice("xian"));
    println!("input=xian context=我想去");
    for candidate in candidates.candidates.iter().take(5) {
        println!(
            "  text={} reading={} schema={} score={:.3}",
            candidate.text, candidate.reading, candidate.schema_id.0, candidate.score
        );
    }

    if let Some((template, trace)) = sample_gesture_trace(&observation_root)? {
        let matches = engine.score_gesture_templates(&trace, 5);
        println!("gesture_template_trace={template}");
        for matched in matches {
            println!(
                "  template={} cost={:.3} samples={}",
                matched.template, matched.cost, matched.samples
            );
        }
        let candidates = engine.decode_gesture_trace(&trace, 5);
        println!("gesture_decode_trace={template}");
        for candidate in candidates.candidates.iter().take(5) {
            println!(
                "  text={} reading={} schema={} score={:.3}",
                candidate.text, candidate.reading, candidate.schema_id.0, candidate.score
            );
        }
    }

    let english_engine = ImeEngine::en_qwerty_from_language_packs(&pack_root)?;
    for input in [
        "keyb",
        "teh",
        "keyboarf",
        "didnt",
        "doesnt",
        "thats",
        "companys",
        "cannot",
        "copyright",
    ] {
        let candidates = english_engine.decode_lattice(&exact_lattice(input));
        println!("en_profile input={input}");
        for candidate in candidates.candidates.iter().take(5) {
            println!(
                "  text={} reading={} schema={} score={:.3}",
                candidate.text, candidate.reading, candidate.schema_id.0, candidate.score
            );
        }
    }

    Ok(())
}

fn sample_gesture_trace(root: &PathBuf) -> Result<Option<(String, Vec<Point>)>, Box<dyn Error>> {
    let pack = ObservationModelPack::load(root)?;
    let Some(templates) = pack.gesture_templates else {
        return Ok(None);
    };
    let Some(template) = templates.templates.first() else {
        return Ok(None);
    };
    let points = template
        .points
        .iter()
        .map(|point| Point::new(point[0] as f32, point[1] as f32))
        .collect::<Vec<_>>();
    Ok(Some((template.word.clone(), points)))
}

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
