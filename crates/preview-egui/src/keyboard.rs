use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Rounding, Stroke, Vec2};
use geometry_core::{GeometryLayout, Point, Rect as GeometryRect};
use geometry_phone_10col::Phone10ColGeometry;
use keymap_core::{KeyLayer, KeyMap};
use keymap_latin_qwerty::LatinQwertyKeyMap;

#[derive(Clone, Debug)]
pub struct KeyView {
    pub ch: char,
    pub geometry: GeometryRect,
}

#[derive(Clone, Debug)]
pub struct KeyboardView {
    keys: Vec<KeyView>,
    bounds: GeometryRect,
}

impl Default for KeyboardView {
    fn default() -> Self {
        let geometry = Phone10ColGeometry::new();
        let keymap = LatinQwertyKeyMap::new();
        let keys = geometry
            .slots()
            .iter()
            .filter_map(|slot| {
                keymap
                    .symbol_for_slot(&slot.id, KeyLayer::Normal)
                    .and_then(|symbol| symbol.0.chars().next())
                    .map(|ch| KeyView {
                        ch,
                        geometry: slot.bounds,
                    })
            })
            .collect::<Vec<_>>();
        let bounds = keys
            .iter()
            .fold(None, |bounds: Option<GeometryRect>, key| {
                Some(match bounds {
                    Some(existing) => union(existing, key.geometry),
                    None => key.geometry,
                })
            })
            .unwrap_or(GeometryRect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            });
        Self { keys, bounds }
    }
}

impl KeyboardView {
    pub fn geometry_from_screen(&self, point: Pos2, rect: Rect) -> Point {
        let x = ((point.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        let y = ((point.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
        Point::new(
            self.bounds.x + x * self.bounds.width,
            self.bounds.y + y * self.bounds.height,
        )
    }

    pub fn screen_from_geometry(&self, point: Point, rect: Rect) -> Pos2 {
        let x = (point.x - self.bounds.x) / self.bounds.width.max(f32::EPSILON);
        let y = (point.y - self.bounds.y) / self.bounds.height.max(f32::EPSILON);
        Pos2::new(
            rect.left() + x * rect.width(),
            rect.top() + y * rect.height(),
        )
    }

    pub fn paint(&self, painter: &egui::Painter, rect: Rect) {
        for key in &self.keys {
            let key_rect = self
                .screen_rect(key.geometry, rect)
                .shrink2(Vec2::new(3.0, 5.0));
            painter.rect(
                key_rect,
                Rounding::same(8.0),
                Color32::from_rgb(246, 248, 251),
                Stroke::new(1.0, Color32::from_rgb(184, 193, 205)),
            );
            painter.text(
                key_rect.center(),
                Align2::CENTER_CENTER,
                key.ch.to_ascii_uppercase().to_string(),
                FontId::proportional(23.0),
                Color32::from_rgb(35, 42, 52),
            );
        }
    }

    fn screen_rect(&self, source: GeometryRect, target: Rect) -> Rect {
        let left = self.screen_from_geometry(Point::new(source.x, source.y), target);
        let right = self.screen_from_geometry(
            Point::new(source.x + source.width, source.y + source.height),
            target,
        );
        Rect::from_two_pos(left, right)
    }
}

fn union(left: GeometryRect, right: GeometryRect) -> GeometryRect {
    let min_x = left.x.min(right.x);
    let min_y = left.y.min(right.y);
    let max_x = (left.x + left.width).max(right.x + right.width);
    let max_y = (left.y + left.height).max(right.y + right.height);
    GeometryRect {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    }
}
