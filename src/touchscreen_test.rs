use evdev::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
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

struct AsciiCanvas {
    w: u16,
    h: u16,
    buf: Vec<char>,
}

impl AsciiCanvas {
    fn new(w: u16, h: u16) -> Self {
        Self {
            w,
            h,
            buf: vec![' '; (w as usize) * (h as usize)],
        }
    }
    fn idx(&self, x: i32, y: i32) -> Option<usize> {
        if x >= 0 && y >= 0 && (x as u16) < self.w && (y as u16) < self.h {
            Some((y as usize) * (self.w as usize) + (x as usize))
        } else {
            None
        }
    }
    fn put(&mut self, x: i32, y: i32, ch: char) {
        if let Some(i) = self.idx(x, y) {
            self.buf[i] = ch;
        }
    }
    fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, ch: char) {
        // Bresenham
        let (mut x0, mut y0) = (x0, y0);
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.put(x0, y0, ch);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }
    fn arrow(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, shaft: char) {
        self.line(x0, y0, x1, y1, shaft);
        // crude arrowhead at (x1,y1)
        let hx = x1 - x0;
        let hy = y1 - y0;
        let (ax, ay, head) = if hx.abs() > hy.abs() {
            if hx >= 0 {
                (x1, y1, '>')
            } else {
                (x1, y1, '<')
            }
        } else {
            if hy >= 0 {
                (x1, y1, 'v')
            } else {
                (x1, y1, '^')
            }
        };
        self.put(ax, ay, head);
    }
    fn crosshair(&mut self, x: i32, y: i32, size: i32, strong: bool) {
        let (hch, vch, cch) = if strong {
            ('-', '|', 'X')
        } else {
            ('-', '|', '+')
        };
        for dx in -size..=size {
            self.put(x + dx, y, hch);
        }
        for dy in -size..=size {
            self.put(x, y + dy, vch);
        }
        self.put(x, y, cch);
    }
    fn text_block(&mut self, x: i32, y: i32, s: &str) {
        for (i, ch) in s.chars().enumerate() {
            self.put(x + i as i32, y, ch);
        }
    }
    fn to_text(&self) -> Text<'_> {
        let mut out = String::with_capacity(self.buf.len() + self.h as usize);
        for row in 0..self.h {
            let start = (row as usize) * (self.w as usize);
            out.push_str(
                &self.buf[start..start + self.w as usize]
                    .iter()
                    .collect::<String>(),
            );
            if row + 1 != self.h {
                out.push('\n');
            }
        }
        Text::from(out)
    }
}

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

    pub fn points_visual(&self) -> Option<Vec<(f64, f64)>> {
        None
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

    fn draw_calibration(&self, f: &mut Frame) {
        let area = f.area();
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);

        // Header
        let header = Paragraph::new(Line::from(vec![
            Span::styled(
                " Touchscreen calibration ",
                Style::default().fg(Color::Cyan).bold(),
            ),
            Span::raw(" – press "),
            Span::styled("Q", Style::default().fg(Color::Yellow).bold()),
            Span::raw(" to exit"),
        ]))
        .centered()
        .block(Block::bordered().title("Touchscreen"));
        f.render_widget(header, chunks[0]);

        // Footer
        let footer =
            Paragraph::new("Touch the glowing corner. Arrows show where to tap next.").centered();
        f.render_widget(footer, chunks[2]);

        // Active corner index based on your state
        use CalibrationStep::*;
        let active = match self.calibration.step {
            TopLeft => 0,
            TopRight => 1,
            BottomRight => 2,
            BottomLeft => 3,
            Done => usize::MAX,
        };

        // Build ASCII scene inside the middle chunk
        let w = chunks[1].width.saturating_sub(2).max(10);
        let h = chunks[1].height.saturating_sub(2).max(6);
        let mut ac = AsciiCanvas::new(w, h);

        // Geometry (local to chunk)
        let cx = (w as i32) / 2;
        let cy = (h as i32) / 2;
        let margin = 2i32;
        let corners = [
            (margin, margin),                                   // TL
            ((w as i32) - 1 - margin, margin),                  // TR
            ((w as i32) - 1 - margin, (h as i32) - 1 - margin), // BR
            (margin, (h as i32) - 1 - margin),                  // BL
        ];

        // Arrows: active corner gets arrow made of '*' (more visible), others '-'
        for (i, &(tx, ty)) in corners.iter().enumerate() {
            let shaft = if i == active { '*' } else { '-' };
            ac.arrow(cx, cy, tx, ty, shaft);
        }

        // Crosshairs: active is stronger/larger
        for (i, &(tx, ty)) in corners.iter().enumerate() {
            let strong = i == active;
            let size = if strong { 3 } else { 2 };
            ac.crosshair(tx, ty, size, strong);
            // tiny target dot
            ac.put(tx, ty, if strong { 'X' } else { '+' });
        }

        // Optional: show user taps if you have them
        if let Some(points) = self.calibration.points_visual() {
            for (x, y) in points {
                // map to local coords if needed; here we assume already local (0..w, 0..h)
                ac.put(x as i32, y as i32, 'o');
            }
        }

        // Center message (ASCII only; styled via Paragraph on top)
        let msg = match self.calibration.step {
            Done => "Calibration complete!",
            _ => "Touch the highlighted corner to calibrate",
        };

        let field = Paragraph::new(ac.to_text()).block(Block::bordered().title("Calibrate"));
        f.render_widget(field, chunks[1]);

        // Overlay centered instruction (kept ASCII-friendly)
        let msg_box_w = (msg.len() as u16 + 6).min(chunks[1].width.saturating_sub(4));
        let msg_box_h = 3u16;
        let instr_rect = Rect {
            x: chunks[1].x + (chunks[1].width.saturating_sub(msg_box_w)) / 2,
            y: chunks[1].y + chunks[1].height / 2 - msg_box_h / 2,
            width: msg_box_w,
            height: msg_box_h,
        };
        let instr = Paragraph::new(Text::from(msg))
            .centered()
            .block(Block::bordered().title("Instructions"));
        f.render_widget(instr, instr_rect);
    }

    fn draw_test(&self, f: &mut Frame) {
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(f.area());

        self.draw_header(f, chunks[0]);

        self.draw_touchmap(f, chunks[1]);

        self.draw_footer(f, chunks[2]);
    }
}

impl Screen for TouchscreenTestScreen {
    fn id(&self) -> ScreenId {
        ScreenId::TouchscreenTest
    }

    fn draw(&self, frame: &mut Frame) {
        if self.calibration.is_done() {
            self.draw_test(frame);
        } else {
            self.draw_calibration(frame);
        }
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
