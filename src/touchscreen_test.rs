use evdev::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Block, Paragraph},
};
use std::u16;

use crate::{Nav, Screen, ScreenId, event_handler::AppEvent};

// Conservative raw-unit thresholds; tweak to your device scale if needed:
static MIN_SPAN_X: u16 = 500; // require at least this many raw units across X
static MIN_SPAN_Y: u16 = 500; // require at least this many raw units across Y
static MIN_CORNER_DIST2: u32 = 100 * 100; // squared distance; avoid identical points (~100 raw units apart)
static MIN_DIAGONAL2: u32 = 100000; // squared distance; reject near-degenerate rectangles (~1000 units)

static EXPECTED_MAX_X: u16 = 1000;
static EXPECTED_MAX_Y: u16 = 1000;

static COLS: u16 = 16;
static ROWS: u16 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CalibrationStep {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
    Done,
}

#[derive(Clone, Debug)]
struct Calibration {
    step: CalibrationStep,
    // recorded points in order TL, TR, BR, BL
    pts: [(u16, u16); 4],
    count: usize,

    // derived mapping
    min_x: u16,
    max_x: u16,
    min_y: u16,
    max_y: u16,
    invert_x: bool,
    invert_y: bool,
    scale_x: f32,
    scale_y: f32,

    is_touching: bool,
    error: Option<String>,
}

impl Calibration {
    fn new() -> Self {
        Self {
            step: CalibrationStep::TopLeft,
            pts: [(0, 0); 4],
            count: 0,
            min_x: 0,
            max_x: EXPECTED_MAX_X,
            min_y: 0,
            max_y: EXPECTED_MAX_Y,
            invert_x: false,
            invert_y: false,
            scale_x: 1.0,
            scale_y: 1.0,
            is_touching: false,
            error: None,
        }
    }

    fn prompt(&self) -> String {
        let prompt_str = if self.is_touching { "Release" } else { "Touch" };

        format!(
            "{} the {}",
            prompt_str,
            match self.step {
                CalibrationStep::TopLeft => "TOP-LEFT",
                CalibrationStep::TopRight => "TOP-RIGHT",
                CalibrationStep::BottomRight => "BOTTOM-RIGHT",
                CalibrationStep::BottomLeft => "BOTTOM-LEFT",
                CalibrationStep::Done => "Calibration complete",
            }
        )
    }

    fn record_touch(&mut self, touch_event: &AppEvent) {
        if let AppEvent::Touch {
            x,
            y,
            timestamp: _,
            released,
        } = touch_event
        {
            if let CalibrationStep::Done = self.step {
                return;
            }

            if !released {
                self.is_touching = true;
                return;
            }

            self.is_touching = false;

            self.pts[self.count] = (*x, *y);
            self.count += 1;
            self.step = match self.step {
                CalibrationStep::TopLeft => CalibrationStep::TopRight,
                CalibrationStep::TopRight => CalibrationStep::BottomRight,
                CalibrationStep::BottomRight => CalibrationStep::BottomLeft,
                CalibrationStep::BottomLeft => CalibrationStep::Done,
                CalibrationStep::Done => CalibrationStep::Done,
            };
            if let CalibrationStep::Done = self.step {
                self.finalize();
                if self.error.is_some() {
                    // Reset to try again
                    self.step = CalibrationStep::TopLeft;
                    self.count = 0;
                }
            }
        }
    }

    fn finalize(&mut self) {
        // Min/max window
        let (mut min_x, mut max_x) = (u16::MAX, 0u16);
        let (mut min_y, mut max_y) = (u16::MAX, 0u16);
        for &(x, y) in &self.pts {
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }

        // Error detections
        if max_x.saturating_sub(min_x) < MIN_SPAN_X {
            self.error = Some(format!(
                "Calibration failed: X span too small ({} < {})",
                max_x - min_x,
                MIN_SPAN_X
            ));
            return;
        }
        if max_y.saturating_sub(min_y) < MIN_SPAN_Y {
            self.error = Some(format!(
                "Calibration failed: Y span too small ({} < {})",
                max_y - min_y,
                MIN_SPAN_Y
            ));
            return;
        }

        // 3) corner uniqueness & diagonal sanity
        let d2 = |a: (u16, u16), b: (u16, u16)| -> u32 {
            let dx = a.0 as i32 - b.0 as i32;
            let dy = a.1 as i32 - b.1 as i32;
            (dx * dx + dy * dy) as u32
        };
        let (tl, tr, br, bl) = (self.pts[0], self.pts[1], self.pts[2], self.pts[3]);

        // corners not almost same point
        let all_pairs = [
            d2(tl, tr),
            d2(tr, br),
            d2(br, bl),
            d2(bl, tl),
            d2(tl, br),
            d2(tr, bl),
        ];
        if all_pairs.iter().any(|&p| p < MIN_CORNER_DIST2) {
            self.error = Some("Calibration failed: some samples too close together.".to_string());
            return;
        }

        // diagonals must be reasonably long
        if d2(tl, br) < MIN_DIAGONAL2 || d2(tr, bl) < MIN_DIAGONAL2 {
            self.error = Some("Calibration failed: total area too small.".to_string());
            return;
        }

        self.min_x = min_x;
        self.max_x = max_x;
        self.min_y = min_y;
        self.max_y = max_y;

        // Detect axis direction using row/column comparisons
        let (tl, tr, br, bl) = (self.pts[0], self.pts[1], self.pts[2], self.pts[3]);
        // X increases left->right?
        self.invert_x = tr.0 < tl.0;
        // Y increases top->bottom?
        let top_y = (tl.1 as u32 + tr.1 as u32) / 2;
        let bottom_y = (bl.1 as u32 + br.1 as u32) / 2;
        self.invert_y = (bottom_y as i64) < (top_y as i64);

        // Avoid div by zero
        let dx = (self.max_x as i32 - self.min_x as i32).max(1) as f32;
        let dy = (self.max_y as i32 - self.min_y as i32).max(1) as f32;

        self.scale_x = (EXPECTED_MAX_X as f32) / dx;
        self.scale_y = (EXPECTED_MAX_Y as f32) / dy;

        self.error = None;
    }

    #[inline]
    fn is_done(&self) -> bool {
        matches!(self.step, CalibrationStep::Done)
    }

    #[inline]
    fn map(&self, raw_x: u16, raw_y: u16) -> (u16, u16) {
        let nx = ((raw_x as i32 - self.min_x as i32) as f32 * self.scale_x)
            .clamp(0.0, EXPECTED_MAX_X as f32);
        let ny = ((raw_y as i32 - self.min_y as i32) as f32 * self.scale_y)
            .clamp(0.0, EXPECTED_MAX_Y as f32);

        let mut x = nx as u16;
        let mut y = ny as u16;

        if self.invert_x {
            x = EXPECTED_MAX_X.saturating_sub(x);
        }
        if self.invert_y {
            y = EXPECTED_MAX_Y.saturating_sub(y);
        }
        (x, y)
    }

    fn progress(&self) -> (usize, usize) {
        (self.count + 1, 4)
    }
}
pub struct TouchscreenTestScreen {
    is_touched: Vec<bool>,
    last_touch: Option<AppEvent>,
    calibration: Calibration,
    touching_idx: Option<usize>,
}

impl TouchscreenTestScreen {
    #[inline]
    fn idx(&self, c: usize, r: usize) -> usize {
        r * (COLS as usize) + c
    }

    pub fn new() -> Self {
        TouchscreenTestScreen {
            is_touched: vec![false; (COLS * ROWS) as usize],
            last_touch: None,
            calibration: Calibration::new(),
            touching_idx: None,
        }
    }

    // Map (raw) -> (calibrated logical)
    fn map_raw(&self, x: u16, y: u16) -> (u16, u16) {
        if self.calibration.is_done() {
            self.calibration.map(x, y)
        } else {
            // During calibration just clamp to logical space so header can display something sane
            (x.min(EXPECTED_MAX_X), y.min(EXPECTED_MAX_Y))
        }
    }

    fn mark(&mut self, x: u16, y: u16) {
        let col = (x * COLS / EXPECTED_MAX_X).min(COLS - 1);
        let row = (y * ROWS / EXPECTED_MAX_Y).min(ROWS - 1);

        let index = self.idx(col as usize, row as usize);
        if index < self.is_touched.len() {
            self.is_touched[index] = true;
        }
    }

    fn coverage_ratio(&self) -> f32 {
        let touched = self.is_touched.iter().filter(|&&c| c).count() as f32;
        touched / (self.is_touched.len() as f32)
    }

    fn draw_header(&self, frame: &mut Frame, area: Rect) {
        let mut text = vec![];

        let mode_text = if self.calibration.is_done() {
            let coverage_percent = (self.coverage_ratio() * 100.0).round() as u8;
            format!("Coverage: {}%", coverage_percent).into()
        } else {
            let (n, d) = self.calibration.progress();
            format!("Calibrating.. {} ({}/{})", self.calibration.prompt(), n, d)
        };

        text.push(mode_text.bold().cyan());

        text.push(" | ".into());
        text.push(
            match &self.last_touch {
                Some(AppEvent::Touch { x, y, .. }) => {
                    format!("Last touch: ({}, {})", x, y)
                }
                _ => "No touch registered".to_string(),
            }
            .gray(),
        );

        if let Some(err) = &self.calibration.error {
            text.push(" | ".into());
            text.push(err.clone().bold().red());
        }

        let title = Line::from(text);

        let p = Paragraph::new(title).block(Block::bordered());
        frame.render_widget(p, area);
    }
    fn draw_footer(&self, frame: &mut Frame, area: Rect) {
        let footer = Line::from(vec![
            "Press ".into(),
            "Q/Esc".bold().yellow(),
            " to quit.".into(),
        ])
        .centered();

        frame.render_widget(footer, area);
    }

    fn draw_touchmap(&self, frame: &mut Frame, area: Rect) {
        if area.width < COLS || area.height < ROWS {
            return;
        }

        let cell_width = area.width / COLS;
        let cell_height = area.height / ROWS;

        let vertical_chunks = Layout::vertical([
            Constraint::Min(0),
            Constraint::Length(cell_height * ROWS),
            Constraint::Min(0),
        ])
        .split(area);

        let horizontal_chunks = Layout::horizontal([
            Constraint::Min(0),
            Constraint::Length(cell_width * COLS),
            Constraint::Min(0),
        ])
        .split(vertical_chunks[1]);

        let touchmap_area = horizontal_chunks[1];

        for row in 0..ROWS {
            for col in 0..COLS {
                let x = touchmap_area.x + col * cell_width;
                let y = touchmap_area.y + row * cell_height;
                let is_touched = self.is_touched[self.idx(col as usize, row as usize)];

                let bg = if is_touched {
                    if Some(self.idx(col as usize, row as usize)) == self.touching_idx {
                        Color::LightRed
                    } else {
                        Color::Green
                    }
                } else {
                    Color::Gray
                };

                let style = Style::default().bg(bg);
                let line = " ".repeat(cell_width as usize);
                let rect = Rect {
                    x,
                    y,
                    width: cell_width,
                    height: cell_height,
                };
                frame.render_widget(Paragraph::new(line.as_str()).style(style), rect);
            }
        }
    }

    fn handle_touch(&mut self, touch_event: AppEvent) {
        if let AppEvent::Touch {
            x,
            y,
            timestamp,
            released,
        } = touch_event
        {
            if self.calibration.is_done() {
                let (mx, my) = self.map_raw(x, y);
                self.mark(mx, my);
                if released {
                    self.touching_idx = None;
                } else {
                    let col = (mx * COLS / EXPECTED_MAX_X).min(COLS - 1);
                    let row = (my * ROWS / EXPECTED_MAX_Y).min(ROWS - 1);
                    let index = self.idx(col as usize, row as usize);
                    self.touching_idx = Some(index);
                }
                self.last_touch = Some(AppEvent::Touch {
                    x: x,
                    y: y,
                    timestamp,
                    released,
                });
            } else {
                self.calibration.record_touch(&touch_event);
                self.last_touch = Some(touch_event);
            }
        }
    }
}

impl Screen for TouchscreenTestScreen {
    fn id(&self) -> ScreenId {
        ScreenId::TouchscreenTest
    }

    fn draw(&self, frame: &mut Frame) {
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(frame.area());

        self.draw_header(frame, chunks[0]);

        self.draw_touchmap(frame, chunks[1]);

        self.draw_footer(frame, chunks[2]);
    }

    fn handle_event(&mut self, event: AppEvent) -> Nav {
        match event {
            AppEvent::Touch { .. } => {
                self.handle_touch(event);
            }
            AppEvent::Key { code, .. } => {
                if code == KeyCode::KEY_Q || code == KeyCode::KEY_ESC {
                    return Nav::To(ScreenId::Home);
                }
            }
            _ => {}
        }

        Nav::Stay
    }
}
