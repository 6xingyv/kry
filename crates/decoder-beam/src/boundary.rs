use schema_core::Boundary;

pub(super) fn boundary_hint_cost(boundary: &Boundary, hints: &[usize]) -> f32 {
    if hints.is_empty() {
        return 0.0;
    }
    let mut offsets = Vec::new();
    let mut offset = 0usize;
    for segment in boundary
        .segments
        .iter()
        .take(boundary.segments.len().saturating_sub(1))
    {
        offset += segment.chars().count();
        offsets.push(offset);
    }

    let mut cost = 0.0;
    for hint in hints {
        if offsets.contains(hint) {
            cost -= 1.5;
        } else {
            cost += 0.75;
        }
    }
    cost
}
