use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SlotId(pub String);

impl SlotId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn distance_to(self, other: Point) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn center(self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    pub fn contains(self, point: Point) -> bool {
        point.x >= self.x
            && point.x <= self.x + self.width
            && point.y >= self.y
            && point.y <= self.y + self.height
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub id: SlotId,
    pub bounds: Rect,
    pub neighbors: Vec<SlotId>,
}

impl Slot {
    pub fn center(&self) -> Point {
        self.bounds.center()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HitTestResult {
    pub slot_id: SlotId,
    pub distance: f32,
    pub probability: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SlotObservation {
    pub slot_id: SlotId,
    pub cost: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SlotLattice {
    pub positions: Vec<Vec<SlotObservation>>,
}

impl SlotLattice {
    pub fn new(positions: Vec<Vec<SlotObservation>>) -> Self {
        Self { positions }
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraceSegment {
    pub start: Point,
    pub end: Point,
    pub direction: f32,
    pub arc_length: f32,
}

pub fn detect_key_turns(points: &[Point], min_angle: f32, min_spacing: f32) -> Vec<usize> {
    if points.len() < 5 {
        return Vec::new();
    }

    let window = 3usize;
    let arcs = cumulative_arc_lengths(points);
    let total_arc = arcs.last().copied().unwrap_or(0.0);
    if total_arc < min_spacing {
        return Vec::new();
    }

    let mut candidates: Vec<(usize, f32)> = Vec::new();
    for i in window..points.len().saturating_sub(window) {
        let back_x = points[i].x - points[i - window].x;
        let back_y = points[i].y - points[i - window].y;
        let fwd_x = points[i + window].x - points[i].x;
        let fwd_y = points[i + window].y - points[i].y;

        let back_len = (back_x * back_x + back_y * back_y).sqrt();
        let fwd_len = (fwd_x * fwd_x + fwd_y * fwd_y).sqrt();
        if back_len < 1e-6 || fwd_len < 1e-6 {
            continue;
        }

        let dot = back_x * fwd_x + back_y * fwd_y;
        let cross = back_x * fwd_y - back_y * fwd_x;
        let angle = cross.atan2(dot).abs();

        if angle >= min_angle {
            candidates.push((i, angle));
        }
    }

    candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut turns = Vec::new();
    for &(idx, _) in &candidates {
        let arc = arcs[idx];
        let too_close = turns
            .iter()
            .any(|&t: &usize| (arcs[t] - arc).abs() < min_spacing);
        if !too_close {
            turns.push(idx);
        }
    }
    turns.sort();
    turns
}

pub fn trace_segments(points: &[Point], turns: &[usize]) -> Vec<TraceSegment> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut boundaries = Vec::with_capacity(turns.len() + 2);
    boundaries.push(0);
    for &t in turns {
        if t > 0 && t < points.len() - 1 {
            boundaries.push(t);
        }
    }
    boundaries.push(points.len() - 1);
    boundaries.dedup();

    let arcs = cumulative_arc_lengths(points);
    boundaries
        .windows(2)
        .map(|w| {
            let (si, ei) = (w[0], w[1]);
            let start = points[si];
            let end = points[ei];
            let dx = end.x - start.x;
            let dy = end.y - start.y;
            TraceSegment {
                start,
                end,
                direction: dy.atan2(dx),
                arc_length: arcs[ei] - arcs[si],
            }
        })
        .collect()
}

fn cumulative_arc_lengths(points: &[Point]) -> Vec<f32> {
    let mut arcs = Vec::with_capacity(points.len());
    arcs.push(0.0);
    for pair in points.windows(2) {
        arcs.push(arcs.last().unwrap() + pair[0].distance_to(pair[1]));
    }
    arcs
}

pub trait GeometryLayout {
    fn slots(&self) -> &[Slot];

    fn slot(&self, id: &SlotId) -> Option<&Slot> {
        self.slots().iter().find(|slot| &slot.id == id)
    }

    fn neighbors(&self, id: &SlotId) -> Vec<SlotId> {
        self.slot(id)
            .map(|slot| slot.neighbors.clone())
            .unwrap_or_default()
    }

    fn hit_test(&self, point: Point, limit: usize) -> Vec<HitTestResult> {
        let mut hits = self
            .slots()
            .iter()
            .map(|slot| {
                let distance = if slot.bounds.contains(point) {
                    0.0
                } else {
                    point.distance_to(slot.center())
                };
                HitTestResult {
                    slot_id: slot.id.clone(),
                    distance,
                    probability: 1.0 / (1.0 + distance),
                }
            })
            .collect::<Vec<_>>();

        hits.sort_by(|a, b| {
            a.distance
                .partial_cmp(&b.distance)
                .unwrap_or(Ordering::Equal)
        });
        hits.truncate(limit);
        hits
    }

    fn slot_lattice_for_points(&self, points: &[Point], alternatives: usize) -> SlotLattice {
        SlotLattice::new(
            points
                .iter()
                .map(|point| {
                    self.hit_test(*point, alternatives)
                        .into_iter()
                        .map(|hit| SlotObservation {
                            slot_id: hit.slot_id,
                            cost: hit.distance,
                        })
                        .collect()
                })
                .collect(),
        )
    }
}
