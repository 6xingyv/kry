use data_core::ObservationModelPack;
use geometry_core::{GeometryLayout, Point, TraceSegment, detect_key_turns, trace_segments};
use keymap_core::{KeyLayer, KeyMap};
use schema_core::IncrementalSchema;
use std::collections::HashMap;

const START_POINT_PREFILTER_MIN: usize = 256;
const START_POINT_PREFILTER_PER_RESULT: usize = 32;
const SCHEMA_GUIDED_MAX_KEYS: usize = 18;

// Structural observation term (E_obs, per math.md "trace → slot lattice").
// A swipe evidences an intended key only where the finger pivots: the start, the
// end, and interior corners. A candidate whose letter count differs from the
// number of pivots is claiming keys the trace never turned at (e.g. the "u" in a
// straight o→r making the bogus "our") or omitting keys the trace clearly turned
// at. Penalizing |letters − pivots| is what discriminates collinear/length-
// ambiguous words that global shape distance scores identically.
// Tuned for normalized [0,1] trace coordinates (key width ≈ 0.1).
const PIVOT_TURN_ANGLE: f32 = 0.80; // ~46°, robust to per-point noise
const PIVOT_TURN_SPACING_FACTOR: f32 = 1.5; // × distance unit (≈ 1.5 key widths)
// Pure tie-breaker: small enough that a real frequency gap (a rare word's entry
// cost ≈ 2) overrides it, so legitimate doubles ("been") / collinear-through
// ("out") still win on frequency; large enough to break exact ties where both
// words floor at schema 0 ("or" vs "our", "we" vs "wee").
const PIVOT_STRUCT_WEIGHT: f32 = 0.15; // per excess/missing key, pre ×8 amplification

// Subsequence decode: a swipe's word is the valid-schema SUBSEQUENCE of the keys
// the finger crossed. Even-index sampling fails on real traces (the finger dwells
// on long traversals like h→a→o, so evenly-spaced samples miss the corners
// n-i-h-a-o). Here we walk every crossed key and let the schema pick which to
// take; corners + endpoints are the intended keys.
const SUBSEQ_SKIP_CORNER_W: f32 = 0.5; // skipping a pivot/endpoint (likely intended)
const SUBSEQ_TAKE_PASS_W: f32 = 1.0; // taking a non-corner passed-over key (likely not)
const SUBSEQ_SEARCH_BUDGET: usize = 8_000;
const SUBSEQ_MAX_EVENTS: usize = 18; // beyond this the trace is too noisy/long; skip
// Only run the (heavier) subsequence pass when even-index sampling failed to find
// a clean match — i.e. its best cost is worse than this. Clean swipes keep the
// fast path AND their correct result; curved/realistic traces get rescued.
const SUBSEQ_GATE_COST: f32 = 0.8;
const SCHEMA_GUIDED_SEARCH_BUDGET: usize = 6_000;

#[derive(Clone, Debug, Default)]
pub(super) struct GestureTemplateStartIndex {
    cell_size: f32,
    by_cell: HashMap<(i32, i32), Vec<usize>>,
}

impl GestureTemplateStartIndex {
    pub(super) fn build(templates: &[RuntimeGestureTemplate], cell_size: f32) -> Self {
        let cell_size = indexed_cell_size(templates, cell_size);
        let mut by_cell: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (index, template) in templates.iter().enumerate() {
            let Some(start) = template.points.first() else {
                continue;
            };
            by_cell
                .entry(start_cell(*start, cell_size))
                .or_default()
                .push(index);
        }
        Self { cell_size, by_cell }
    }

    pub(super) fn is_indexed(&self) -> bool {
        !self.by_cell.is_empty()
    }

    pub(super) fn nearby_indices(&self, point: Point) -> Vec<usize> {
        if self.by_cell.is_empty() {
            return Vec::new();
        }
        let center = start_cell(point, self.cell_size);
        let mut indices = Vec::new();
        for radius in 0i32..=2 {
            for dx in -radius..=radius {
                for dy in -radius..=radius {
                    if dx.abs() != radius && dy.abs() != radius {
                        continue;
                    }
                    if let Some(cell_indices) = self.by_cell.get(&(center.0 + dx, center.1 + dy)) {
                        indices.extend(cell_indices.iter().copied());
                    }
                }
            }
            if !indices.is_empty() {
                break;
            }
        }
        indices
    }
}

fn indexed_cell_size(templates: &[RuntimeGestureTemplate], model_unit: f32) -> f32 {
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for start in templates
        .iter()
        .filter_map(|template| template.points.first().copied())
    {
        min_x = min_x.min(start.x);
        max_x = max_x.max(start.x);
        min_y = min_y.min(start.y);
        max_y = max_y.max(start.y);
    }
    let longest_axis = (max_x - min_x).max(max_y - min_y);
    let data_unit = if longest_axis.is_finite() && longest_axis > 0.0 {
        longest_axis / 64.0
    } else {
        f32::INFINITY
    };
    model_unit.min(data_unit).max(0.0001)
}

#[derive(Clone, Debug, PartialEq)]
pub struct GestureTemplateMatch {
    pub template: String,
    pub cost: f32,
    pub samples: usize,
}

fn start_cell(point: Point, cell_size: f32) -> (i32, i32) {
    (
        (point.x / cell_size).floor() as i32,
        (point.y / cell_size).floor() as i32,
    )
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RuntimeGestureTemplate {
    pub(super) template: String,
    pub(super) samples: usize,
    pub(super) points: Vec<Point>,
}

pub(super) fn gesture_templates_from_pack(
    pack: &ObservationModelPack,
) -> Vec<RuntimeGestureTemplate> {
    pack.gesture_templates
        .as_ref()
        .map(|artifact| {
            artifact
                .templates
                .iter()
                .filter_map(|template| {
                    let points = template
                        .points
                        .iter()
                        .filter_map(|point| {
                            let x = point[0];
                            let y = point[1];
                            (x.is_finite() && y.is_finite()).then(|| Point::new(x as f32, y as f32))
                        })
                        .collect::<Vec<_>>();
                    (!template.word.is_empty() && !points.is_empty()).then(|| {
                        let point_count = points.len();
                        RuntimeGestureTemplate {
                            template: template.word.clone(),
                            samples: template.count,
                            points: resample_polyline(&points, point_count),
                        }
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn score_gesture_templates_against<'a>(
    templates: impl IntoIterator<Item = &'a RuntimeGestureTemplate>,
    points: &[Point],
    limit: usize,
    distance_unit: f32,
) -> Vec<GestureTemplateMatch> {
    if points.is_empty() || limit == 0 {
        return Vec::new();
    }

    let unit = distance_unit.max(0.0001);
    let observed_start = points[0];
    let mut start_ranked = templates
        .into_iter()
        .filter_map(|template| {
            template
                .points
                .first()
                .map(|start| (template, observed_start.distance_to(*start)))
        })
        .collect::<Vec<_>>();
    start_ranked.sort_by(|a, b| {
        a.1.total_cmp(&b.1)
            .then_with(|| b.0.samples.cmp(&a.0.samples))
            .then_with(|| a.0.template.cmp(&b.0.template))
    });
    let search_budget = start_ranked
        .len()
        .min(START_POINT_PREFILTER_MIN.max(limit.saturating_mul(START_POINT_PREFILTER_PER_RESULT)));

    let mut resampled_by_len: HashMap<usize, Vec<Point>> = HashMap::new();
    let mut matches = start_ranked
        .into_iter()
        .take(search_budget)
        .filter_map(|template| {
            let template = template.0;
            let key_count = template.template.len();
            let v2_cost = gesture_path_cost_v2(points, &template.points, key_count) / unit;
            let resampled = resampled_by_len
                .entry(template.points.len())
                .or_insert_with(|| resample_polyline(points, template.points.len()));
            let v1_cost = gesture_path_cost(resampled.as_slice(), &template.points) / unit;
            let dtw_cost = dtw_mean_distance(points, &template.points) / unit;
            let cost = median3(v2_cost, v1_cost, dtw_cost);
            cost.is_finite().then(|| GestureTemplateMatch {
                template: template.template.clone(),
                cost,
                samples: template.samples,
            })
        })
        .collect::<Vec<_>>();

    matches.sort_by(|a, b| {
        a.cost
            .total_cmp(&b.cost)
            .then_with(|| b.samples.cmp(&a.samples))
            .then_with(|| a.template.cmp(&b.template))
    });
    matches.truncate(limit);
    matches
}

fn resample_polyline(points: &[Point], target_count: usize) -> Vec<Point> {
    if points.is_empty() || target_count == 0 {
        return Vec::new();
    }
    if target_count == 1 || points.len() == 1 {
        return vec![points[0]; target_count];
    }

    let mut cumulative = Vec::with_capacity(points.len());
    cumulative.push(0.0);
    for pair in points.windows(2) {
        cumulative.push(cumulative.last().copied().unwrap_or(0.0) + pair[0].distance_to(pair[1]));
    }

    let Some(total) = cumulative.last().copied() else {
        return Vec::new();
    };
    if total <= f32::EPSILON {
        return vec![points[0]; target_count];
    }

    let mut result = Vec::with_capacity(target_count);
    let mut segment = 0usize;
    for index in 0..target_count {
        let target = if target_count == 1 {
            0.0
        } else {
            total * index as f32 / (target_count - 1) as f32
        };
        while segment + 1 < cumulative.len() && cumulative[segment + 1] < target {
            segment += 1;
        }
        if segment + 1 >= points.len() {
            result.push(*points.last().unwrap());
            continue;
        }
        let start_distance = cumulative[segment];
        let end_distance = cumulative[segment + 1];
        let span = (end_distance - start_distance).max(f32::EPSILON);
        let t = (target - start_distance) / span;
        result.push(lerp_point(points[segment], points[segment + 1], t));
    }
    result
}

fn lerp_point(start: Point, end: Point, t: f32) -> Point {
    Point::new(
        start.x + (end.x - start.x) * t,
        start.y + (end.y - start.y) * t,
    )
}

fn mean_point_distance(lhs: &[Point], rhs: &[Point]) -> f32 {
    if lhs.len() != rhs.len() || lhs.is_empty() {
        return f32::INFINITY;
    }
    lhs.iter()
        .zip(rhs)
        .map(|(left, right)| left.distance_to(*right))
        .sum::<f32>()
        / lhs.len() as f32
}

fn gesture_path_cost(lhs: &[Point], rhs: &[Point]) -> f32 {
    if lhs.len() != rhs.len() || lhs.is_empty() {
        return f32::INFINITY;
    }
    let mean = mean_point_distance(lhs, rhs);
    let endpoint = lhs
        .last()
        .zip(rhs.last())
        .map(|(left, right)| left.distance_to(*right))
        .unwrap_or(f32::INFINITY);
    let tail_count = lhs.len().min(4);
    let tail_start = lhs.len() - tail_count;
    let tail = mean_point_distance(&lhs[tail_start..], &rhs[tail_start..]);
    mean + endpoint * 0.75 + tail * 0.5
}

fn dtw_mean_distance(a: &[Point], b: &[Point]) -> f32 {
    let na = a.len().min(64);
    let nb = b.len().min(64);
    let sa = resample_polyline(a, na);
    let sb = resample_polyline(b, nb);

    let mut prev = vec![f32::INFINITY; nb + 1];
    let mut curr = vec![f32::INFINITY; nb + 1];
    prev[0] = 0.0;

    for i in 1..=na {
        curr[0] = f32::INFINITY;
        for j in 1..=nb {
            let d = sa[i - 1].distance_to(sb[j - 1]);
            curr[j] = d + prev[j].min(curr[j - 1]).min(prev[j - 1]);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[nb] / (na.max(nb)) as f32
}

const V2_TURN_ANGLE: f32 = std::f32::consts::FRAC_PI_4;
const V2_TURN_SPACING: f32 = 8.0;

/// Median of three path-cost metrics. We deliberately avoid `min`: a dropped or
/// substituted key is cheap under whichever single metric is most forgiving
/// (usually the segment-aligned v2), and `min` would let that one metric rescue
/// a geometrically wrong path. The median demands that at least two of the three
/// metrics agree the path is cheap, so a near-miss must fool a majority to win.
fn median3(a: f32, b: f32, c: f32) -> f32 {
    let mut v = [a, b, c];
    v.sort_by(|x, y| x.total_cmp(y));
    v[1]
}

pub(super) fn gesture_path_cost_v2(trace: &[Point], template: &[Point], key_count: usize) -> f32 {
    if trace.len() < 2 || template.len() < 2 {
        return f32::INFINITY;
    }

    let n = key_count.max(1) as f32;

    let trace_turns = detect_key_turns(trace, V2_TURN_ANGLE, V2_TURN_SPACING);
    let template_turns = detect_key_turns(template, V2_TURN_ANGLE * 0.7, V2_TURN_SPACING);

    let trace_segs = trace_segments(trace, &trace_turns);
    let template_segs = trace_segments(template, &template_turns);

    let segment_cost = if !trace_segs.is_empty()
        && !template_segs.is_empty()
        && trace_segs.len() == template_segs.len()
    {
        aligned_segment_cost(&trace_segs, &template_segs)
    } else {
        dtw_mean_distance(trace, template)
    };

    let start_err = trace[0].distance_to(template[0]);
    let end_err = trace
        .last()
        .zip(template.last())
        .map(|(a, b)| a.distance_to(*b))
        .unwrap_or(0.0);

    (segment_cost + start_err * 0.3 + end_err * 0.5) / n
}

fn aligned_segment_cost(trace: &[TraceSegment], template: &[TraceSegment]) -> f32 {
    let mut cost = 0.0f32;
    for (ts, tpl) in trace.iter().zip(template) {
        let dir_diff = angle_diff(ts.direction, tpl.direction).abs();
        let len_ratio = if tpl.arc_length > 1e-6 {
            (ts.arc_length / tpl.arc_length - 1.0).abs()
        } else {
            ts.arc_length
        };
        let endpoint_err = ts.end.distance_to(tpl.end);
        cost += dir_diff * 0.4 + len_ratio * 0.3 + endpoint_err * 0.3;
    }
    cost / trace.len().max(1) as f32
}

fn angle_diff(a: f32, b: f32) -> f32 {
    let mut d = a - b;
    while d > std::f32::consts::PI {
        d -= 2.0 * std::f32::consts::PI;
    }
    while d < -std::f32::consts::PI {
        d += 2.0 * std::f32::consts::PI;
    }
    d
}

fn geometry_distance_unit(geometry: &dyn GeometryLayout) -> f32 {
    let slots = geometry.slots();
    if slots.is_empty() {
        return 0.05;
    }
    let mean_w: f32 = slots.iter().map(|s| s.bounds.width).sum::<f32>() / slots.len() as f32;
    (mean_w * 0.5).max(0.001)
}

pub(super) fn schema_guided_gesture_decode(
    trace: &[Point],
    geometry: &dyn GeometryLayout,
    keymap: &dyn KeyMap,
    schema: &dyn IncrementalSchema,
    beam_budget: usize,
    _distance_unit: f32,
) -> Vec<GestureTemplateMatch> {
    if trace.len() < 2 || beam_budget == 0 {
        return Vec::new();
    }

    let unit = geometry_distance_unit(geometry);
    let top_k = 3usize;

    // Number of keys the trace actually evidences: start + end + interior pivots
    // (corners). Used by the structural E_obs term in the DFS leaf.
    let n_pivots = 2 + detect_key_turns(trace, PIVOT_TURN_ANGLE, unit * PIVOT_TURN_SPACING_FACTOR).len();

    let char_centers: HashMap<char, Point> = geometry
        .slots()
        .iter()
        .filter_map(|slot| {
            keymap
                .symbol_for_slot(&slot.id, KeyLayer::Normal)
                .and_then(|sym| sym.0.chars().next().map(|ch| (ch, slot.center())))
        })
        .collect();

    let min_keys = 2usize;
    let max_keys = ((trace.len() + 3) / 4).clamp(min_keys, SCHEMA_GUIDED_MAX_KEYS);
    let mut budget = SCHEMA_GUIDED_SEARCH_BUDGET;

    schema.reset_arena();

    let mut all_results: Vec<GestureTemplateMatch> = Vec::new();

    for n_keys in min_keys..=max_keys {
        if budget == 0 {
            break;
        }
        let sampled = proportional_sample_by_index(trace, n_keys);

        let events: Vec<Vec<(char, f32)>> = sampled
            .iter()
            .map(|pos| {
                let mut hits: Vec<(char, f32)> = geometry
                    .hit_test(*pos, top_k)
                    .into_iter()
                    .filter_map(|hit| {
                        let sym = keymap.symbol_for_slot(&hit.slot_id, KeyLayer::Normal)?;
                        let ch = sym.0.chars().next()?;
                        Some((ch, hit.distance / unit))
                    })
                    .collect();
                hits.sort_by(|a, b| a.1.total_cmp(&b.1));
                hits
            })
            .collect();

        let init = schema.initial_state();
        let mut path = String::with_capacity(n_keys);
        gesture_dfs(
            &events,
            schema,
            &init,
            &mut path,
            0,
            &char_centers,
            trace,
            unit,
            n_pivots,
            &mut all_results,
            &mut budget,
        );
    }

    // Subsequence decode: the best valid-schema subsequence of the crossed keys.
    // Robust where even sampling fails (long traversals hide the real corners like
    // 你好's n-i-h-a-o). Gated to only run when even sampling found nothing clean.
    let best_even = all_results
        .iter()
        .map(|r| r.cost)
        .fold(f32::INFINITY, f32::min);
    let corner_set: std::collections::HashSet<usize> = if best_even > SUBSEQ_GATE_COST {
        detect_key_turns(trace, PIVOT_TURN_ANGLE, unit * PIVOT_TURN_SPACING_FACTOR)
            .into_iter()
            .collect()
    } else {
        std::collections::HashSet::new()
    };
    let events = if best_even > SUBSEQ_GATE_COST {
        build_pass_events(trace, geometry, keymap, &corner_set, unit)
    } else {
        Vec::new()
    };
    if events.len() >= 2 && events.len() <= SUBSEQ_MAX_EVENTS {
        let total_corners = events.iter().filter(|e| e.is_corner).count();
        let init = schema.initial_state();
        let mut taken = String::new();
        let mut subseq_budget = SUBSEQ_SEARCH_BUDGET;
        subseq_gesture_dfs(
            &events,
            schema,
            &init,
            &mut taken,
            0,
            0,
            0.0,
            total_corners,
            &mut all_results,
            &mut subseq_budget,
            0,
        );
    }

    all_results.sort_by(|a, b| a.cost.total_cmp(&b.cost));
    all_results.dedup_by(|a, b| a.template == b.template);
    all_results.truncate(beam_budget.max(32));
    all_results
}

/// A key the finger crossed (consecutive duplicates collapsed), with the closest
/// approach distance and whether the finger pivoted there (corner) or it's an
/// endpoint — i.e. an intended key vs one merely passed over.
struct PassEvent {
    ch: char,
    dist: f32,
    is_corner: bool,
}

fn build_pass_events(
    trace: &[Point],
    geometry: &dyn GeometryLayout,
    keymap: &dyn KeyMap,
    corner_set: &std::collections::HashSet<usize>,
    unit: f32,
) -> Vec<PassEvent> {
    // Intended keys only = endpoints + detected corners. Restricting to these keeps
    // the subsequence search tiny (a few events, not every crossed key — that made
    // it 2^N), and on an ideal trace every letter IS a corner. Collinear-through
    // keys (rare) are covered by the even-sampling path that runs alongside.
    let mut indices: Vec<usize> = Vec::with_capacity(corner_set.len() + 2);
    indices.push(0);
    indices.extend(corner_set.iter().copied());
    indices.push(trace.len() - 1);
    indices.sort_unstable();
    indices.dedup();

    let mut events: Vec<PassEvent> = Vec::new();
    for &i in &indices {
        let Some(hit) = geometry.hit_test(trace[i], 1).into_iter().next() else {
            continue;
        };
        let Some(sym) = keymap.symbol_for_slot(&hit.slot_id, KeyLayer::Normal) else {
            continue;
        };
        let Some(ch) = sym.0.chars().next() else {
            continue;
        };
        let dist = hit.distance / unit;
        if let Some(last) = events.last_mut() {
            if last.ch == ch {
                last.dist = last.dist.min(dist);
                continue;
            }
        }
        events.push(PassEvent {
            ch,
            dist,
            is_corner: true,
        });
    }
    events
}

#[allow(clippy::too_many_arguments)]
fn subseq_gesture_dfs(
    events: &[PassEvent],
    schema: &dyn IncrementalSchema,
    state: &schema_core::SchemaStateId,
    taken: &mut String,
    taken_count: usize,
    corners_taken: usize,
    accum_dist: f32,
    total_corners: usize,
    results: &mut Vec<GestureTemplateMatch>,
    budget: &mut usize,
    depth: usize,
) {
    if *budget == 0 {
        return;
    }
    *budget = budget.saturating_sub(1);

    // Record at every valid word boundary (shorter words pay for the corners /
    // endpoints they leave uncovered, so full-coverage words win).
    if taken_count >= 2 && !schema.candidates_at(state).is_empty() {
        let skipped_corners = total_corners.saturating_sub(corners_taken);
        let taken_noncorner = taken_count - corners_taken;
        let cost = accum_dist
            + skipped_corners as f32 * SUBSEQ_SKIP_CORNER_W
            + taken_noncorner as f32 * SUBSEQ_TAKE_PASS_W;
        if cost.is_finite() {
            results.push(GestureTemplateMatch {
                template: taken.clone(),
                cost,
                samples: 0,
            });
        }
    }

    if depth == events.len() {
        return;
    }
    let ev = &events[depth];

    // TAKE this key (advance the schema), if it keeps a valid path alive.
    for adv in schema.advance(state, ev.ch) {
        if *budget == 0 {
            return;
        }
        if adv.next_state.alive || !adv.completed.is_empty() {
            taken.push(ev.ch);
            subseq_gesture_dfs(
                events,
                schema,
                &adv.next_state,
                taken,
                taken_count + 1,
                corners_taken + ev.is_corner as usize,
                accum_dist + ev.dist,
                total_corners,
                results,
                budget,
                depth + 1,
            );
            taken.pop();
        }
    }

    // SKIP this key (a passed-over key not part of the word).
    subseq_gesture_dfs(
        events,
        schema,
        state,
        taken,
        taken_count,
        corners_taken,
        accum_dist,
        total_corners,
        results,
        budget,
        depth + 1,
    );
}

fn gesture_dfs(
    events: &[Vec<(char, f32)>],
    schema: &dyn IncrementalSchema,
    state: &schema_core::SchemaStateId,
    path: &mut String,
    depth: usize,
    char_centers: &HashMap<char, Point>,
    trace: &[Point],
    unit: f32,
    n_pivots: usize,
    results: &mut Vec<GestureTemplateMatch>,
    budget: &mut usize,
) {
    if *budget == 0 {
        return;
    }
    *budget = budget.saturating_sub(1);

    if depth == events.len() {
        if path.len() < 2 || schema.candidates_at(state).is_empty() {
            return;
        }
        let ideal: Vec<Point> = path
            .chars()
            .filter_map(|ch| char_centers.get(&ch).copied())
            .collect();
        if ideal.len() != path.len() {
            return;
        }
        let ideal_dense = build_ideal_trace_dense(&ideal, trace.len());
        let mean_cost = gesture_path_cost(trace, &ideal_dense) / unit;
        let v2_cost = gesture_path_cost_v2(trace, &ideal_dense, path.len()) / unit;
        let dtw_cost = dtw_mean_distance(trace, &ideal_dense) / unit;
        // Shape distance (how well the word's path matches the trace) plus the
        // structural pivot term (whether the word has the right number of
        // intended keys). Shape alone scores collinear words like or/our equal;
        // the pivot term separates them.
        // A doubled letter ("soon", "been") is a real key that leaves no corner,
        // so discount adjacent repeats before comparing letter count to pivots —
        // otherwise legitimate doubles get penalized like the bogus "u" in "our".
        let key_count = path.chars().count();
        let repeats = path
            .chars()
            .zip(path.chars().skip(1))
            .filter(|(a, b)| a == b)
            .count();
        let effective_keys = key_count.saturating_sub(repeats);
        let pivot_mismatch = (effective_keys as i32 - n_pivots as i32).unsigned_abs() as f32;
        let cost = median3(mean_cost, v2_cost, dtw_cost) + pivot_mismatch * PIVOT_STRUCT_WEIGHT;
        if cost.is_finite() {
            results.push(GestureTemplateMatch {
                template: path.clone(),
                cost,
                samples: 0,
            });
        }
        return;
    }

    for &(ch, _obs_cost) in &events[depth] {
        if *budget == 0 {
            return;
        }
        for adv in schema.advance(state, ch) {
            if !adv.next_state.alive && adv.completed.is_empty() {
                continue;
            }
            if adv.next_state.alive {
                path.push(ch);
                gesture_dfs(
                    events,
                    schema,
                    &adv.next_state,
                    path,
                    depth + 1,
                    char_centers,
                    trace,
                    unit,
                    n_pivots,
                    results,
                    budget,
                );
                path.pop();
            }
        }
    }
}

fn proportional_sample_by_index(trace: &[Point], n: usize) -> Vec<Point> {
    if n == 0 || trace.is_empty() {
        return Vec::new();
    }
    if n == 1 || trace.len() == 1 {
        return vec![trace[0]];
    }
    (0..n)
        .map(|i| {
            let idx_f = i as f32 * (trace.len() - 1) as f32 / (n - 1) as f32;
            let idx = idx_f.floor() as usize;
            let frac = idx_f - idx as f32;
            if idx + 1 >= trace.len() {
                trace[trace.len() - 1]
            } else {
                lerp_point(trace[idx], trace[idx + 1], frac)
            }
        })
        .collect()
}

/// Debug helper: returns the three component costs `(v1, v2, dtw)` (each already
/// divided by `unit`) for an ideal key sequence against a trace, exactly as the
/// DFS leaf computes them. Lets diagnostics see which metric is too lenient.
pub fn debug_path_costs(trace: &[Point], ideal_keys: &[Point], unit: f32) -> (f32, f32, f32) {
    if ideal_keys.len() < 2 || trace.len() < 2 {
        return (f32::INFINITY, f32::INFINITY, f32::INFINITY);
    }
    let ideal_dense = build_ideal_trace_dense(ideal_keys, trace.len());
    let v1 = gesture_path_cost(trace, &ideal_dense) / unit;
    let v2 = gesture_path_cost_v2(trace, &ideal_dense, ideal_keys.len()) / unit;
    let dtw = dtw_mean_distance(trace, &ideal_dense) / unit;
    (v1, v2, dtw)
}

fn build_ideal_trace_dense(keys: &[Point], n_points: usize) -> Vec<Point> {
    if keys.len() < 2 || n_points < 2 {
        return keys.to_vec();
    }
    let n_segments = keys.len() - 1;
    let pts_per_seg = (n_points - 1) / n_segments;
    let remainder = (n_points - 1) % n_segments;

    let mut result = Vec::with_capacity(n_points);
    result.push(keys[0]);

    for seg in 0..n_segments {
        let n = pts_per_seg + if seg < remainder { 1 } else { 0 };
        for i in 1..=n {
            let t = i as f32 / n as f32;
            result.push(lerp_point(keys[seg], keys[seg + 1], t));
        }
    }

    result
}
