use std::sync::mpsc::{self, Receiver, Sender};

use decoder_core::CandidateList;
use engine_core::ImeEngine;
use geometry_core::{GeometryLayout, Point};
use keymap_core::KeyMap;

use crate::config::PreviewConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LanguageMode {
    Zh,
    En,
}

impl LanguageMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Zh => "中文",
            Self::En => "English",
        }
    }
}

#[derive(Clone, Debug)]
pub enum DecodeRequest {
    DecodeTaps {
        id: u64,
        language: LanguageMode,
        symbols: String,
    },
    DecodeGesture {
        id: u64,
        language: LanguageMode,
        points: Vec<Point>,
    },
    SetContext {
        language: LanguageMode,
        text: String,
    },
    AcceptSwipe {
        language: LanguageMode,
        text: String,
    },
    ResetSwipe {
        language: LanguageMode,
    },
}

#[derive(Clone, Debug)]
pub struct DecodeResponse {
    pub id: u64,
    pub candidates: CandidateList,
}

pub struct PreviewDecoder {
    tx: Sender<DecodeRequest>,
    rx: Receiver<DecodeResponse>,
    next_id: u64,
    pending_id: u64,
    awaiting_response: bool,
}

impl PreviewDecoder {
    pub fn new(config: PreviewConfig) -> Self {
        let (tx, worker_rx) = mpsc::channel();
        let (worker_tx, rx) = mpsc::channel();
        std::thread::spawn(move || run_worker(config, worker_rx, worker_tx));
        Self {
            tx,
            rx,
            next_id: 1,
            pending_id: 0,
            awaiting_response: false,
        }
    }

    pub fn decode_taps(&mut self, language: LanguageMode, symbols: String) {
        let id = self.alloc_id();
        let _ = self.tx.send(DecodeRequest::DecodeTaps {
            id,
            language,
            symbols,
        });
    }

    pub fn decode_gesture(&mut self, language: LanguageMode, points: Vec<Point>) {
        let id = self.alloc_id();
        let _ = self.tx.send(DecodeRequest::DecodeGesture {
            id,
            language,
            points,
        });
    }

    pub fn set_context(&self, language: LanguageMode, text: String) {
        let _ = self.tx.send(DecodeRequest::SetContext { language, text });
    }

    pub fn accept_swipe(&self, language: LanguageMode, text: String) {
        let _ = self.tx.send(DecodeRequest::AcceptSwipe { language, text });
    }

    pub fn reset_swipe(&self, language: LanguageMode) {
        let _ = self.tx.send(DecodeRequest::ResetSwipe { language });
    }

    pub fn take_latest(&mut self) -> Option<CandidateList> {
        let mut latest = None;
        while let Ok(response) = self.rx.try_recv() {
            if response.id >= self.pending_id {
                self.pending_id = response.id;
                self.awaiting_response = false;
                latest = Some(response.candidates);
            }
        }
        latest
    }

    pub fn is_waiting(&self) -> bool {
        self.awaiting_response
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.pending_id = id;
        self.awaiting_response = true;
        id
    }
}

fn run_worker(config: PreviewConfig, rx: Receiver<DecodeRequest>, tx: Sender<DecodeResponse>) {
    let mut zh = ImeEngine::zh_qwerty_from_artifacts(
        &config.language_pack_root,
        &config.observation_pack_root,
    )
    .expect("load zh engine");
    let mut en = ImeEngine::en_qwerty_from_artifacts(
        &config.language_pack_root,
        &config.observation_pack_root,
    )
    .expect("load en engine");

    while let Ok(request) = rx.recv() {
        match request {
            DecodeRequest::DecodeTaps {
                id,
                language,
                symbols,
            } => {
                let engine = engine_for(&mut zh, &mut en, language);
                let candidates = key_points(&symbols)
                    .map(|points| engine.decode_taps(&points))
                    .unwrap_or_default();
                let _ = tx.send(DecodeResponse { id, candidates });
            }
            DecodeRequest::DecodeGesture {
                id,
                language,
                points,
            } => {
                let engine = engine_for(&mut zh, &mut en, language);
                let candidates = engine.decode_gesture_trace(&points, 8);
                let _ = tx.send(DecodeResponse { id, candidates });
            }
            DecodeRequest::SetContext { language, text } => {
                engine_for(&mut zh, &mut en, language).set_committed_context(text);
            }
            DecodeRequest::AcceptSwipe { language, text } => {
                engine_for(&mut zh, &mut en, language).accept_swipe_candidate(&text);
            }
            DecodeRequest::ResetSwipe { language } => {
                engine_for(&mut zh, &mut en, language).reset_swipe_session();
            }
        }
    }
}

fn engine_for<'a>(
    zh: &'a mut ImeEngine,
    en: &'a mut ImeEngine,
    language: LanguageMode,
) -> &'a mut ImeEngine {
    match language {
        LanguageMode::Zh => zh,
        LanguageMode::En => en,
    }
}

pub fn symbols_from_points(points: &[Point]) -> String {
    raw_symbols_from_points(points)
}

pub fn preview_symbols_from_points(points: &[Point]) -> String {
    let simplified = simplify_points(points, 0.055);
    let preview = raw_symbols_from_points(&simplified);
    if preview.is_empty() {
        raw_symbols_from_points(points)
    } else {
        preview
    }
}

fn raw_symbols_from_points(points: &[Point]) -> String {
    let mut symbols = String::new();
    let mut last_slot = None;
    let geometry = geometry_phone_10col::Phone10ColGeometry::new();
    let keymap = keymap_latin_qwerty::LatinQwertyKeyMap::new();
    for point in points {
        let top_slot = geometry
            .hit_test(*point, 1)
            .into_iter()
            .next()
            .map(|hit| hit.slot_id);
        if top_slot.is_some() && top_slot != last_slot {
            if let Some(symbol) = top_slot
                .as_ref()
                .and_then(|slot| keymap.symbol_for_slot(slot, keymap_core::KeyLayer::Normal))
            {
                symbols.push_str(&symbol.0);
            }
            last_slot = top_slot;
        }
    }
    symbols
}

fn key_points(symbols: &str) -> Option<Vec<Point>> {
    let geometry = geometry_phone_10col::Phone10ColGeometry::new();
    let keymap = keymap_latin_qwerty::LatinQwertyKeyMap::new();
    symbols
        .chars()
        .map(|ch| {
            keymap
                .slot_for_symbol(ch.to_ascii_lowercase())
                .and_then(|slot| geometry.slot(&slot).map(|slot| slot.center()))
        })
        .collect()
}

fn simplify_points(points: &[Point], epsilon: f32) -> Vec<Point> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    simplify_range(points, 0, points.len() - 1, epsilon, &mut keep);
    points
        .iter()
        .zip(keep)
        .filter_map(|(point, keep)| keep.then_some(*point))
        .collect()
}

fn simplify_range(points: &[Point], start: usize, end: usize, epsilon: f32, keep: &mut [bool]) {
    if end <= start + 1 {
        return;
    }
    let mut best_index = start;
    let mut best_distance = 0.0;
    for index in start + 1..end {
        let distance = point_line_distance(points[index], points[start], points[end]);
        if distance > best_distance {
            best_distance = distance;
            best_index = index;
        }
    }
    if best_distance > epsilon {
        keep[best_index] = true;
        simplify_range(points, start, best_index, epsilon, keep);
        simplify_range(points, best_index, end, epsilon, keep);
    }
}

fn point_line_distance(point: Point, start: Point, end: Point) -> f32 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq <= f32::EPSILON {
        return point.distance_to(start);
    }
    let t = (((point.x - start.x) * dx + (point.y - start.y) * dy) / len_sq).clamp(0.0, 1.0);
    let projected = Point::new(start.x + dx * t, start.y + dy * t);
    point.distance_to(projected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_simplifies_nji_transit_to_ni() {
        let points = key_points("nji").unwrap();
        assert_eq!(preview_symbols_from_points(&points), "ni");
    }

    #[test]
    fn zh_glide_nji_decodes_ni_candidate() {
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let language_root = workspace_root.join("assets/language-packs");
        let observation_root =
            workspace_root.join("assets/observation-models/geometry-phone-10col/qwerty");
        let engine = ImeEngine::zh_qwerty_from_artifacts(language_root, observation_root).unwrap();
        let points = key_points("nji").unwrap();
        let candidates = engine.decode_gesture_trace(&points, 8);
        assert!(
            candidates
                .candidates
                .iter()
                .take(8)
                .any(|candidate| candidate.text == "你"),
            "expected 你 for N-J-I glide, got {:?}",
            candidates
                .candidates
                .iter()
                .take(8)
                .map(|candidate| (&candidate.text, &candidate.reading, candidate.score))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn zh_glide_curved_nihao_decodes_nihao_candidate() {
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let language_root = workspace_root.join("assets/language-packs");
        let observation_root =
            workspace_root.join("assets/observation-models/geometry-phone-10col/qwerty");
        let engine = ImeEngine::zh_qwerty_from_artifacts(language_root, observation_root).unwrap();
        let points = curved_points_through("nihao");
        let candidates = engine.decode_gesture_trace(&points, 8);
        assert!(
            candidates
                .candidates
                .iter()
                .take(8)
                .any(|candidate| candidate.text == "你好"),
            "expected 你好 for curved nihao glide, got {:?}",
            candidates
                .candidates
                .iter()
                .take(8)
                .map(|candidate| (&candidate.text, &candidate.reading, candidate.score))
                .collect::<Vec<_>>()
        );
    }

    fn curved_points_through(symbols: &str) -> Vec<Point> {
        let centers = key_points(symbols).unwrap();
        let mut points = Vec::new();
        for (segment_index, pair) in centers.windows(2).enumerate() {
            let start = pair[0];
            let end = pair[1];
            if segment_index == 0 {
                points.push(start);
            }
            for step in 1..=8 {
                let t = step as f32 / 8.0;
                let bend = (std::f32::consts::PI * t).sin() * 0.035;
                points.push(Point::new(
                    start.x + (end.x - start.x) * t,
                    start.y + (end.y - start.y) * t + bend,
                ));
            }
        }
        points
    }
}
