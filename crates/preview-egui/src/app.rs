use eframe::egui::{
    self, Color32, FontId, Pos2, Rect, Sense, Stroke, TextEdit, TopBottomPanel, Vec2,
};
use geometry_core::Point;
use std::time::Duration;

use crate::config::PreviewConfig;
use crate::decoder::{
    LanguageMode, PreviewDecoder, preview_symbols_from_points, symbols_from_points,
};
use crate::keyboard::KeyboardView;

const TOUCH_SAMPLE_STEP: f32 = 0.012;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputMode {
    Slide,
    Physical,
}

impl InputMode {
    fn label(self) -> &'static str {
        match self {
            Self::Slide => "滑行",
            Self::Physical => "物理键盘",
        }
    }
}

pub struct PreviewApp {
    decoder: PreviewDecoder,
    keyboard: KeyboardView,
    language: LanguageMode,
    input_mode: InputMode,
    stroke: Vec<Point>,
    tracking: bool,
    preview_symbols: String,
    typed_symbols: String,
    committed: String,
    candidates: Vec<decoder_core::Candidate>,
}

impl PreviewApp {
    pub fn new(config: PreviewConfig) -> Self {
        Self {
            decoder: PreviewDecoder::new(config),
            keyboard: KeyboardView::default(),
            language: LanguageMode::Zh,
            input_mode: InputMode::Slide,
            stroke: Vec::new(),
            tracking: false,
            preview_symbols: String::new(),
            typed_symbols: String::new(),
            committed: String::new(),
            candidates: Vec::new(),
        }
    }

    fn clear_composition(&mut self) {
        self.stroke.clear();
        self.preview_symbols.clear();
        self.typed_symbols.clear();
        self.candidates.clear();
        self.tracking = false;
    }

    fn apply_results(&mut self) {
        if let Some(list) = self.decoder.take_latest() {
            self.candidates = list.candidates.into_iter().take(12).collect();
        }
    }

    fn commit_candidate(&mut self, index: usize) {
        let Some(candidate) = self.candidates.get(index) else {
            return;
        };
        self.committed.push_str(&candidate.text);
        if self.input_mode == InputMode::Slide {
            self.decoder
                .accept_swipe(self.language, candidate.text.clone());
        }
        self.decoder
            .set_context(self.language, self.committed.clone());
        self.clear_composition();
    }

    fn decode_current_taps(&mut self) {
        if self.typed_symbols.is_empty() {
            self.candidates.clear();
            return;
        }
        self.decoder
            .decode_taps(self.language, self.typed_symbols.clone());
    }

    fn process_physical_keyboard(&mut self, ctx: &egui::Context) {
        let mut changed = false;
        let events = ctx.input(|input| input.events.clone());
        for event in events {
            match event {
                egui::Event::Text(text) => {
                    for ch in text.chars().filter(|ch| ch.is_ascii_alphabetic()) {
                        self.typed_symbols.push(ch.to_ascii_lowercase());
                        changed = true;
                    }
                }
                egui::Event::Key {
                    key: egui::Key::Backspace,
                    pressed: true,
                    ..
                } => {
                    self.typed_symbols.pop();
                    changed = true;
                }
                egui::Event::Key {
                    key: egui::Key::Enter,
                    pressed: true,
                    ..
                } => {
                    self.commit_candidate(0);
                }
                egui::Event::Key {
                    key: egui::Key::Escape,
                    pressed: true,
                    ..
                } => self.clear_composition(),
                _ => {}
            }
        }
        if changed {
            self.decode_current_taps();
        }
    }

    fn process_slide(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, keyboard_rect: Rect) {
        let response = ui.allocate_rect(keyboard_rect, Sense::click_and_drag());
        let pointer_pos = ctx.input(|input| input.pointer.interact_pos());

        if response.drag_started() {
            self.stroke.clear();
            self.preview_symbols.clear();
            self.candidates.clear();
            self.tracking = true;
            if let Some(pos) = ctx
                .input(|input| input.pointer.press_origin())
                .or(pointer_pos)
            {
                if keyboard_rect.expand(24.0).contains(pos) {
                    self.push_stroke_point(pos, keyboard_rect);
                }
            }
        }
        if response.dragged() || self.tracking {
            if let Some(pos) = pointer_pos {
                if keyboard_rect.expand(24.0).contains(pos) {
                    self.push_stroke_point(pos, keyboard_rect);
                    self.preview_symbols = preview_symbols_from_points(&self.stroke);
                }
            }
            ctx.request_repaint();
        }
        let ended_drag = response.drag_stopped() && self.tracking;
        if ended_drag {
            self.tracking = false;
            self.decoder
                .decode_gesture(self.language, self.stroke.clone());
        }
        if response.clicked() && !self.tracking && !ended_drag {
            if let Some(pos) = response.interact_pointer_pos() {
                self.clear_composition();
                self.push_stroke_point(pos, keyboard_rect);
                self.preview_symbols = preview_symbols_from_points(&self.stroke);
                self.typed_symbols = self.preview_symbols.clone();
                self.decode_current_taps();
            }
        }
    }

    fn push_stroke_point(&mut self, pos: Pos2, keyboard_rect: Rect) {
        let point = self.keyboard.geometry_from_screen(pos, keyboard_rect);
        if self
            .stroke
            .last()
            .is_none_or(|last| distance(*last, point) >= TOUCH_SAMPLE_STEP)
        {
            self.stroke.push(point);
        }
    }

    fn composition_text(&self) -> String {
        match self.input_mode {
            InputMode::Slide if !self.tracking => String::new(),
            InputMode::Slide if self.language == LanguageMode::Zh => self.preview_symbols.clone(),
            InputMode::Slide => symbols_from_points(&self.stroke),
            InputMode::Physical => self.typed_symbols.clone(),
        }
    }
}

impl eframe::App for PreviewApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_results();
        if self.decoder.is_waiting() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        if self.input_mode == InputMode::Physical {
            self.process_physical_keyboard(ctx);
        }

        TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                for language in [LanguageMode::Zh, LanguageMode::En] {
                    if ui
                        .selectable_label(self.language == language, language.label())
                        .clicked()
                    {
                        self.language = language;
                        self.clear_composition();
                    }
                }
                ui.separator();
                for input_mode in [InputMode::Slide, InputMode::Physical] {
                    if ui
                        .selectable_label(self.input_mode == input_mode, input_mode.label())
                        .clicked()
                    {
                        self.input_mode = input_mode;
                        self.clear_composition();
                    }
                }
                ui.separator();
                if ui.button("清空").clicked() {
                    self.clear_composition();
                    self.committed.clear();
                    self.decoder.reset_swipe(self.language);
                    self.decoder
                        .set_context(self.language, self.committed.clone());
                }
            });
        });

        TopBottomPanel::bottom("candidates")
            .resizable(false)
            .exact_height(132.0)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let candidates = self.candidates.clone();
                    for (index, candidate) in candidates.iter().enumerate() {
                        let label = format!("{}  {:.2}", candidate.text, candidate.score);
                        if ui.button(label).clicked() {
                            self.commit_candidate(index);
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    let mut composing = self.composition_text();
                    ui.add_enabled(
                        false,
                        TextEdit::singleline(&mut composing)
                            .desired_width(220.0)
                            .font(FontId::proportional(22.0)),
                    );
                    let mut committed = self.committed.clone();
                    ui.add_enabled(
                        false,
                        TextEdit::singleline(&mut committed).desired_width(f32::INFINITY),
                    );
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_rect_before_wrap();
            let keyboard_rect = Rect::from_min_size(
                available.left_bottom() - Vec2::new(0.0, 282.0),
                Vec2::new(available.width(), 268.0),
            )
            .shrink2(Vec2::new(18.0, 14.0));

            if self.input_mode == InputMode::Slide {
                self.process_slide(ctx, ui, keyboard_rect);
            } else {
                ui.allocate_rect(keyboard_rect, Sense::hover());
            }

            let painter = ui.painter();
            self.keyboard.paint(painter, keyboard_rect);
            paint_stroke(painter, keyboard_rect, &self.keyboard, &self.stroke);
        });
    }
}

fn paint_stroke(
    painter: &egui::Painter,
    keyboard_rect: Rect,
    keyboard: &KeyboardView,
    stroke: &[Point],
) {
    for pair in stroke.windows(2) {
        painter.line_segment(
            [
                keyboard.screen_from_geometry(pair[0], keyboard_rect),
                keyboard.screen_from_geometry(pair[1], keyboard_rect),
            ],
            Stroke::new(3.0, Color32::from_rgb(33, 119, 255)),
        );
    }
    for point in stroke {
        painter.circle_filled(
            keyboard.screen_from_geometry(*point, keyboard_rect),
            4.0,
            Color32::from_rgb(33, 119, 255),
        );
    }
}

fn distance(left: Point, right: Point) -> f32 {
    ((left.x - right.x).powi(2) + (left.y - right.y).powi(2)).sqrt()
}
