use geometry_core::{Rect, Slot, SlotId};

#[derive(Clone, Debug)]
pub struct Phone10ColGeometry {
    slots: Vec<Slot>,
}

impl Phone10ColGeometry {
    pub fn new() -> Self {
        let rows = [10usize, 9, 7];
        let row_offsets = [0.0f32, 0.05, 0.15];
        let key_w = 0.1;
        let key_h = 0.25;
        let row_gap = 0.04;

        let mut slots = Vec::new();
        for (row, cols) in rows.iter().enumerate() {
            for col in 0..*cols {
                let id = SlotId::new(format!("r{row}c{col}"));
                let mut neighbors = Vec::new();
                for dr in -1i32..=1 {
                    for dc in -1i32..=1 {
                        if dr == 0 && dc == 0 {
                            continue;
                        }
                        let nr = row as i32 + dr;
                        if !(0..rows.len() as i32).contains(&nr) {
                            continue;
                        }
                        let nc = col as i32 + dc;
                        if (0..rows[nr as usize] as i32).contains(&nc) {
                            neighbors.push(SlotId::new(format!("r{nr}c{nc}")));
                        }
                    }
                }
                slots.push(Slot {
                    id,
                    bounds: Rect {
                        x: row_offsets[row] + col as f32 * key_w,
                        y: row as f32 * (key_h + row_gap),
                        width: key_w,
                        height: key_h,
                    },
                    neighbors,
                });
            }
        }

        Self { slots }
    }
}

impl Default for Phone10ColGeometry {
    fn default() -> Self {
        Self::new()
    }
}

impl geometry_core::GeometryLayout for Phone10ColGeometry {
    fn slots(&self) -> &[Slot] {
        &self.slots
    }
}
