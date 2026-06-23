use decoder_core::CandidateList;
use geometry_core::Point;
use observation_core::{ObservationChunk, ObservationPoint, RawInputEvent};

use crate::{ImeEngine, merge_candidate_lists, observation_model::calibrate_observation_lattice};

impl ImeEngine {
    pub fn reset_stream(&mut self) {
        self.pending_stream_points.clear();
        self.pending_gesture_points.clear();
        self.pending_stream_pause_positions.clear();
        self.pending_stream_last_slot = None;
        self.pending_stream_candidates = CandidateList::default();
        self.pending_gesture_scored_stream_len = 0;
    }

    pub fn feed_observation_chunk(&mut self, chunk: &ObservationChunk) -> CandidateList {
        let mut stream_changed = false;
        for event in &chunk.events {
            match event {
                RawInputEvent::Tap(point) => {
                    stream_changed |= self.push_stream_point(*point, false);
                }
                RawInputEvent::Trace(points) => {
                    for point in points {
                        self.pending_gesture_points
                            .push(Point::new(point.x, point.y));
                        stream_changed |= self.push_stream_point(*point, true);
                    }
                }
                RawInputEvent::Pause { duration_ms } => {
                    if *duration_ms >= 160 && !self.pending_stream_points.is_empty() {
                        let position = self.pending_stream_points.len();
                        if !self.pending_stream_pause_positions.contains(&position) {
                            self.pending_stream_pause_positions.push(position);
                            stream_changed = true;
                        }
                    }
                }
                RawInputEvent::Commit { text } => {
                    self.set_committed_context(text.clone());
                    self.reset_stream();
                    self.reset_swipe_session();
                    stream_changed = true;
                }
                RawInputEvent::Delete { count } => {
                    for _ in 0..*count {
                        if self.pending_stream_points.pop().is_some() {
                            stream_changed = true;
                        }
                    }
                    self.pending_gesture_points.clear();
                    self.pending_stream_pause_positions
                        .retain(|position| *position <= self.pending_stream_points.len());
                    self.pending_stream_last_slot = None;
                    self.pending_stream_candidates = CandidateList::default();
                    self.pending_gesture_scored_stream_len = 0;
                }
                RawInputEvent::HardwareKey { .. } => {}
                RawInputEvent::AcceptCandidate { text, .. } => {
                    self.accept_swipe_candidate(text);
                    self.pending_gesture_points.clear();
                    self.pending_gesture_scored_stream_len = 0;
                }
            }
        }

        if self.pending_stream_points.is_empty() {
            self.pending_stream_candidates = CandidateList::default();
            return CandidateList::default();
        }
        if !stream_changed {
            return self.pending_stream_candidates.clone();
        }

        let lattice = self
            .geometry
            .slot_lattice_for_points(&self.pending_stream_points, 3);
        let calibrated = calibrate_observation_lattice(
            &lattice,
            self.observation_distance_unit,
            &self.observation_slot_units,
        );
        let lattice_candidates = self
            .decode_lattice_with_boundary_hints(&calibrated, &self.pending_stream_pause_positions);
        if self.pending_gesture_points.len() < 2
            || self.pending_stream_points.len() == self.pending_gesture_scored_stream_len
        {
            self.pending_stream_candidates = lattice_candidates;
            return self.pending_stream_candidates.clone();
        }
        let gesture_candidates = self.decode_gesture_trace(&self.pending_gesture_points, 8);
        self.pending_gesture_scored_stream_len = self.pending_stream_points.len();
        self.pending_stream_candidates =
            merge_candidate_lists([lattice_candidates, gesture_candidates]);
        self.pending_stream_candidates.clone()
    }

    fn push_stream_point(
        &mut self,
        point: ObservationPoint,
        collapse_repeated_slots: bool,
    ) -> bool {
        let geometry_point = Point::new(point.x, point.y);
        let top_slot = self
            .geometry
            .hit_test(geometry_point, 1)
            .into_iter()
            .next()
            .map(|hit| hit.slot_id);
        if collapse_repeated_slots
            && top_slot.is_some()
            && self.pending_stream_last_slot.as_ref() == top_slot.as_ref()
        {
            return false;
        }
        self.pending_stream_points.push(geometry_point);
        self.pending_stream_last_slot = top_slot;
        true
    }
}
