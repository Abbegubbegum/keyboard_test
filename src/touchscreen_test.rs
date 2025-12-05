use evdev::KeyCode;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Paragraph},
};
use std::collections::VecDeque;
use std::u16;

use crate::{
    Nav, Screen, ScreenId,
    event_handler::{AppEvent, DeviceInfo},
};

// Conservative raw-unit thresholds; tweak to your device scale if needed:
static MIN_SPAN_X: u16 = 100; // require at least this many raw units across X
static MIN_SPAN_Y: u16 = 100; // require at least this many raw units across Y
static MIN_CORNER_DIST2: u32 = 50 * 50; // squared distance; avoid identical points (~100 raw units apart)
static MIN_DIAGONAL2: u32 = 1000; // squared distance; reject near-degenerate rectangles (~1000 units)

static COLS: u16 = 16;
static ROWS: u16 = 12;

static CALIBRATED_MAX_X: u16 = 999;
static CALIBRATED_MAX_Y: u16 = 999;

// Trail and statistics configuration
const MAX_TRAIL_LENGTH: usize = 200;
const TRAIL_LIFETIME_MS: u128 = 2000; // Trail points disappear after 2 seconds
const JUMP_THRESHOLD: f32 = 50.0; // Distance in units to consider a "jump"

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
    DeviceSelection,
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

    // Hold tracking for calibration
    touch_start_time: Option<u128>,
    touch_start_pos: Option<(u16, u16)>,
    hold_duration_ms: u128,
    touch_samples: Vec<(u16, u16)>, // Collect samples during hold

    // Device selection
    available_devices: Vec<DeviceInfo>,
    selected_device_index: usize,
    selected_device_path: Option<String>,
    selected_device_info: Option<DeviceInfo>,
}

impl Calibration {
    fn new() -> Self {
        Self {
            step: CalibrationStep::DeviceSelection,
            pts: [(0, 0); 4],
            count: 0,
            min_x: 0,
            max_x: u16::MAX,
            min_y: 0,
            max_y: u16::MAX,
            invert_x: false,
            invert_y: false,
            scale_x: 1.0,
            scale_y: 1.0,
            is_touching: false,
            error: None,
            touch_start_time: None,
            touch_start_pos: None,
            hold_duration_ms: 0,
            touch_samples: Vec::new(),
            available_devices: Vec::new(),
            selected_device_index: 0,
            selected_device_path: None,
            selected_device_info: None,
        }
    }

    fn record_touch(&mut self, touch_event: &AppEvent) {
        if let AppEvent::Touch {
            x,
            y,
            timestamp: _,
            released,
            info: _,
        } = touch_event
        {
            if let CalibrationStep::Done = self.step {
                return;
            }

            const REQUIRED_HOLD_MS: u128 = 1000; // 1 second
            const MOVEMENT_TOLERANCE_PERCENT: f32 = 0.025; // 2.5% of device max coordinate
            const MIN_TOLERANCE: i32 = 100; // Minimum tolerance fallback

            if !released {
                // Touch started or continuing
                self.is_touching = true;

                if self.touch_start_time.is_none() {
                    // First touch - record start time and position using system time
                    let current_time = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis();
                    self.touch_start_time = Some(current_time);
                    self.touch_start_pos = Some((*x, *y));
                    self.hold_duration_ms = 0;
                    self.touch_samples.clear();
                    self.touch_samples.push((*x, *y));
                } else {
                    // Continuing touch - check if moved too much
                    if let Some((start_x, start_y)) = self.touch_start_pos {
                        let dx = (*x as i32 - start_x as i32).abs();
                        let dy = (*y as i32 - start_y as i32).abs();

                        // Calculate adaptive threshold based on device's reported maximum coordinates
                        // This is much more reliable than observing coordinates during calibration
                        let max_movement = if let Some(device_info) = &self.selected_device_info {
                            // Use the larger of X or Y max, and apply percentage tolerance
                            let device_max = device_info.abs_x_max.max(device_info.abs_y_max).unwrap_or(1000);
                            ((device_max as f32) * MOVEMENT_TOLERANCE_PERCENT).max(MIN_TOLERANCE as f32) as i32
                        } else {
                            MIN_TOLERANCE // Fallback if device info not available
                        };

                        if dx > max_movement || dy > max_movement {
                            // Moved too much - reset the timer
                            let current_time = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_millis();
                            self.touch_start_time = Some(current_time);
                            self.touch_start_pos = Some((*x, *y));
                            self.hold_duration_ms = 0;
                            self.touch_samples.clear();
                            self.touch_samples.push((*x, *y));
                        } else {
                            // Still within acceptable range - add sample
                            self.touch_samples.push((*x, *y));
                        }
                    }
                }
                // Hold duration will be updated by update_hold_duration() called on Tick events
                return;
            }

            // Touch released
            self.is_touching = false;

            // Check if hold was long enough
            if self.hold_duration_ms >= REQUIRED_HOLD_MS {
                // Calculate average of all samples for better accuracy
                if !self.touch_samples.is_empty() {
                    let sum_x: u32 = self.touch_samples.iter().map(|(x, _)| *x as u32).sum();
                    let sum_y: u32 = self.touch_samples.iter().map(|(_, y)| *y as u32).sum();
                    let count = self.touch_samples.len() as u32;

                    let avg_x = (sum_x / count) as u16;
                    let avg_y = (sum_y / count) as u16;

                    self.pts[self.count] = (avg_x, avg_y);
                    self.count += 1;
                    self.step = match self.step {
                        CalibrationStep::DeviceSelection => CalibrationStep::DeviceSelection, // Should not get touches during device selection
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

            // Reset hold tracking
            self.touch_start_time = None;
            self.touch_start_pos = None;
            self.hold_duration_ms = 0;
            self.touch_samples.clear();
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

        self.scale_x = (CALIBRATED_MAX_X as f32) / dx;
        self.scale_y = (CALIBRATED_MAX_Y as f32) / dy;

        self.error = None;
    }

    #[inline]
    fn is_done(&self) -> bool {
        matches!(self.step, CalibrationStep::Done)
    }

    fn get_hold_progress(&self) -> f32 {
        const REQUIRED_HOLD_MS: u128 = 1000;
        if self.is_touching && self.hold_duration_ms > 0 {
            (self.hold_duration_ms as f32 / REQUIRED_HOLD_MS as f32).min(1.0)
        } else {
            0.0
        }
    }

    fn is_hold_complete(&self) -> bool {
        const REQUIRED_HOLD_MS: u128 = 1000;
        self.is_touching && self.hold_duration_ms >= REQUIRED_HOLD_MS
    }

    fn update_hold_duration(&mut self) {
        // Update hold duration based on current time
        if self.is_touching {
            if let Some(start_time) = self.touch_start_time {
                let current_time = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis();
                self.hold_duration_ms = current_time.saturating_sub(start_time);
            }
        }
    }

    #[inline]
    fn map(&self, raw_x: u16, raw_y: u16) -> (u16, u16) {
        let nx = ((raw_x as i32 - self.min_x as i32) as f32 * self.scale_x)
            .clamp(0.0, CALIBRATED_MAX_X as f32);
        let ny = ((raw_y as i32 - self.min_y as i32) as f32 * self.scale_y)
            .clamp(0.0, CALIBRATED_MAX_Y as f32);

        let mut x = nx as u16;
        let mut y = ny as u16;

        if self.invert_x {
            x = CALIBRATED_MAX_X.saturating_sub(x);
        }
        if self.invert_y {
            y = CALIBRATED_MAX_Y.saturating_sub(y);
        }
        (x, y)
    }
}

#[derive(Clone)]
struct TouchPoint {
    x: u16,
    y: u16,
    timestamp: u128, // Changed to u128 to match SystemTime milliseconds
}

struct TouchStatistics {
    max_jump: f32,
    total_jumps: u32,
    total_samples: u32,
}

impl TouchStatistics {
    fn new() -> Self {
        Self {
            max_jump: 0.0,
            total_jumps: 0,
            total_samples: 0,
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }
}

pub struct TouchscreenTestScreen {
    is_touched: Vec<bool>,
    last_touch: Option<AppEvent>,
    calibration: Calibration,
    touching_idx: Option<usize>,

    // New high-precision features
    trail: VecDeque<TouchPoint>,
    current_touch: Option<TouchPoint>,
    statistics: TouchStatistics,
    last_position: Option<(u16, u16)>,
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
            trail: VecDeque::with_capacity(MAX_TRAIL_LENGTH),
            current_touch: None,
            statistics: TouchStatistics::new(),
            last_position: None,
        }
    }

    // Map (raw) -> (calibrated logical)
    fn map_raw(&self, x: u16, y: u16) -> (u16, u16) {
        if self.calibration.is_done() {
            self.calibration.map(x, y)
        } else {
            // During calibration just clamp to logical space so header can display something sane
            (x.min(CALIBRATED_MAX_X), y.min(CALIBRATED_MAX_Y))
        }
    }

    fn mark(&mut self, x: u16, y: u16) {
        let col = (x * COLS / CALIBRATED_MAX_X).min(COLS - 1);
        let row = (y * ROWS / CALIBRATED_MAX_Y).min(ROWS - 1);

        let index = self.idx(col as usize, row as usize);
        if index < self.is_touched.len() {
            self.is_touched[index] = true;
        }
    }

    fn handle_touch(&mut self, touch_event: AppEvent) {
        if let AppEvent::Touch {
            x,
            y,
            timestamp,
            released,
            ref info,
        } = touch_event
        {
            // During device selection, collect device info from touch events
            if self.calibration.step == CalibrationStep::DeviceSelection {
                if let Some(device_info) = info {
                    // Check if this device is already in the list
                    if !self
                        .calibration
                        .available_devices
                        .iter()
                        .any(|d| d.path == device_info.path)
                    {
                        self.calibration.available_devices.push(device_info.clone());
                    }
                }
                return;
            }

            // After device selection, filter by selected device
            if let Some(selected_path) = &self.calibration.selected_device_path {
                if let Some(device_info) = info {
                    if &device_info.path != selected_path {
                        // Ignore touches from other devices
                        return;
                    }
                }
            }

            if self.calibration.is_done() {
                let (mx, my) = self.map_raw(x, y);

                // Update statistics
                self.statistics.total_samples += 1;

                // Detect jumps
                if let Some((last_x, last_y)) = self.last_position {
                    let dx = mx as f32 - last_x as f32;
                    let dy = my as f32 - last_y as f32;
                    let distance = (dx * dx + dy * dy).sqrt();

                    if distance > JUMP_THRESHOLD {
                        self.statistics.total_jumps += 1;
                        self.statistics.max_jump = self.statistics.max_jump.max(distance);
                    }
                }

                if released {
                    self.current_touch = None;
                    self.last_position = None;
                } else {
                    // Update current touch position and add to trail
                    let current_time = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis();

                    let point = TouchPoint {
                        x: mx,
                        y: my,
                        timestamp: current_time,
                    };

                    // Add to trail on each touch event
                    self.trail.push_back(point.clone());
                    if self.trail.len() > MAX_TRAIL_LENGTH {
                        self.trail.pop_front();
                    }

                    self.current_touch = Some(point);
                    self.last_position = Some((mx, my));
                }

                // Legacy grid marking
                self.mark(mx, my);
                if released {
                    self.touching_idx = None;
                } else {
                    let col = (mx * COLS / CALIBRATED_MAX_X).min(COLS - 1);
                    let row = (my * ROWS / CALIBRATED_MAX_Y).min(ROWS - 1);
                    let index = self.idx(col as usize, row as usize);
                    self.touching_idx = Some(index);
                }

                self.last_touch = Some(AppEvent::Touch {
                    x,
                    y,
                    timestamp,
                    released,
                    info: info.clone(),
                });
            } else {
                self.calibration.record_touch(&touch_event);
                self.last_touch = Some(touch_event);
            }
        }
    }

    fn draw_device_selection(&self, f: &mut Frame) {
        let area = f.area();

        let mut info_lines = vec![
            Line::from(vec![Span::styled(
                "Touchscreen Device Selection",
                Style::default().bold().cyan(),
            )])
            .centered(),
            Line::from(""),
            Line::from("Select which touchscreen/touchpad device to use:")
                .centered()
                .yellow(),
            Line::from(""),
        ];

        // Display the list of available devices
        if self.calibration.available_devices.is_empty() {
            info_lines.push(
                Line::from(vec![Span::styled(
                    "No touch devices detected from touch events.",
                    Style::default().red(),
                )])
                .centered(),
            );
            info_lines.push(Line::from(""));
            info_lines.push(
                Line::from("Try touching the screen to detect devices...")
                    .centered()
                    .gray(),
            );
        } else {
            for (idx, device) in self.calibration.available_devices.iter().enumerate() {
                let is_selected = idx == self.calibration.selected_device_index;
                let marker = if is_selected { "► " } else { "  " };

                let line = Line::from(vec![
                    Span::styled(marker, Style::default().yellow().bold()),
                    Span::styled(
                        format!("{}. {}", idx + 1, device.name),
                        if is_selected {
                            Style::default().bold().yellow()
                        } else {
                            Style::default().white()
                        },
                    ),
                ]);

                info_lines.push(line.centered());
            }
        }

        info_lines.push(Line::from(""));
        info_lines.push(Line::from(""));

        if !self.calibration.available_devices.is_empty() {
            info_lines.push(
                Line::from(vec![
                    Span::styled("↑/↓", Style::default().bold().yellow()),
                    Span::raw(" to navigate   "),
                    Span::styled("Enter", Style::default().bold().yellow()),
                    Span::raw(" to select   "),
                ])
                .centered(),
            );
            info_lines.push(
                Line::from(vec![
                    Span::styled("1-9", Style::default().bold().yellow()),
                    Span::raw(" for quick select"),
                ])
                .centered(),
            );
            info_lines.push(Line::from(""));
        }

        info_lines.push(
            Line::from(vec![
                Span::styled("Q/Esc", Style::default().bold().yellow()),
                Span::raw(" to exit"),
            ])
            .centered(),
        );

        let info_height = info_lines.len() as u16 + 2;
        let info_width = 60u16.min(area.width - 4);

        let info_rect = Rect {
            x: (area.width.saturating_sub(info_width)) / 2,
            y: (area.height.saturating_sub(info_height)) / 2,
            width: info_width,
            height: info_height,
        };

        let info_widget = Paragraph::new(info_lines)
            .block(Block::bordered())
            .style(Style::default().bg(Color::Black).fg(Color::White));

        f.render_widget(info_widget, info_rect);
    }

    fn draw_calibration(&self, f: &mut Frame) {
        let area = f.area();

        // Fill entire screen with canvas (like the test screen)
        let w = area.width;
        let h = area.height;
        let mut ac = AsciiCanvas::new(w, h);

        // Handle device selection separately
        use CalibrationStep::*;
        if self.calibration.step == DeviceSelection {
            self.draw_device_selection(f);
            return;
        }

        // Determine which corner to highlight
        let (target_x, target_y) = match self.calibration.step {
            DeviceSelection => return, // Already handled above
            TopLeft => (0i32, 0i32),
            TopRight => ((w - 1) as i32, 0i32),
            BottomRight => ((w - 1) as i32, (h - 1) as i32),
            BottomLeft => (0i32, (h - 1) as i32),
            Done => (w as i32 / 2, h as i32 / 2), // Center if done
        };

        // Draw arrow from center to the target corner (only if not done)
        if self.calibration.step != Done {
            let cx = (w as i32) / 2;
            let cy = (h as i32) / 2;
            ac.arrow(cx, cy, target_x, target_y, '*');
        }

        // Draw large corner marker at the target corner (after arrow so it overlays)
        if self.calibration.step != Done {
            let size = 7i32;
            for dx in -size..=size {
                ac.put(target_x + dx, target_y, '═');
            }
            for dy in -size..=size {
                ac.put(target_x, target_y + dy, '║');
            }
            ac.put(target_x, target_y, '╬');
        }

        // Render the full-screen canvas
        let canvas_widget =
            Paragraph::new(ac.to_text()).style(Style::default().bg(Color::Black).fg(Color::White));
        f.render_widget(canvas_widget, area);

        // Overlay instruction box at top center
        let msg = match self.calibration.step {
            DeviceSelection => "Select a device", // Should not reach here
            Done => "Calibration complete!",
            TopLeft => "Touch the TOP-LEFT corner of your screen",
            TopRight => "Touch the TOP-RIGHT corner of your screen",
            BottomRight => "Touch the BOTTOM-RIGHT corner of your screen",
            BottomLeft => "Touch the BOTTOM-LEFT corner of your screen",
        };

        let mut info_lines = vec![
            Line::from(vec![Span::styled(
                "Touchscreen Calibration",
                Style::default().bold().cyan(),
            )])
            .centered(),
            Line::from(""),
            Line::from(msg).centered().bold().yellow(),
            Line::from(""),
        ];

        // Show hold progress if touching
        let hold_progress = self.calibration.get_hold_progress();
        let hold_complete = self.calibration.is_hold_complete();

        if hold_complete {
            // Timer complete - show "Release" message
            info_lines.push(
                Line::from(vec![Span::styled(
                    ">>> RELEASE NOW <<<",
                    Style::default().bold().green(),
                )])
                .centered(),
            );
            info_lines.push(Line::from(""));
        } else if hold_progress > 0.0 {
            // Still holding - show progress bar
            let progress_pct = (hold_progress * 100.0) as u8;
            let bar_width = 30;
            let filled = ((hold_progress * bar_width as f32) as usize).min(bar_width);
            let empty = bar_width - filled;

            let progress_bar = format!(
                "[{}{}] {}%",
                "█".repeat(filled),
                "░".repeat(empty),
                progress_pct
            );

            info_lines.push(
                Line::from(vec![
                    Span::styled("Hold: ", Style::default().bold()),
                    Span::styled(progress_bar, Style::default().yellow()),
                ])
                .centered(),
            );
            info_lines.push(Line::from(""));
        } else {
            // Not holding - keep space reserved so the box doesn't resize
            info_lines.push(Line::from(""));
            info_lines.push(Line::from(""));
        }

        info_lines.push(
            Line::from(vec![Span::styled(
                "Touch and HOLD for 1 second ",
                Style::default(),
            )])
            .centered(),
        );
        info_lines.push(
            Line::from(vec![
                Span::styled("Touch the ", Style::default()),
                Span::styled("EDGE OF THE SCREEN", Style::default().bold().yellow()),
            ])
            .centered(),
        );
        info_lines.push(
            Line::from("Touch as close to the physical screen edge as possible")
                .centered()
                .gray(),
        );

        if let Some(AppEvent::Touch { x, y, .. }) = &self.last_touch {
            info_lines.push(
                Line::from(vec![
                    Span::raw("Touch: "),
                    Span::styled(format!("({}, {})", x, y), Style::default().yellow()),
                ])
                .centered(),
            );
        }

        // Show error if present
        if let Some(err) = &self.calibration.error {
            info_lines.push(Line::from(""));
            info_lines.push(
                Line::from(vec![
                    Span::styled("Error: ", Style::default().bold().red()),
                    Span::styled(err.clone(), Style::default().red()),
                ])
                .centered(),
            );
        }

        info_lines.push(Line::from(""));
        info_lines.push(
            Line::from(vec![
                Span::styled("Q/Esc", Style::default().bold().yellow()),
                Span::raw(" to exit"),
            ])
            .centered(),
        );

        let info_height = info_lines.len() as u16 + 2;
        let info_width = 60u16.min(area.width - 4);

        let info_rect = Rect {
            x: (area.width.saturating_sub(info_width)) / 2,
            y: 1,
            width: info_width,
            height: info_height,
        };

        let info_widget = Paragraph::new(info_lines)
            .block(Block::bordered())
            .style(Style::default().bg(Color::Black).fg(Color::White));

        f.render_widget(info_widget, info_rect);
    }

    fn draw_test(&self, f: &mut Frame) {
        // Draw canvas filling the ENTIRE screen first
        self.draw_high_precision_canvas(f, f.area());

        // Overlay UI elements on top of the canvas
        self.draw_overlay_ui(f);
    }

    fn draw_overlay_ui(&self, f: &mut Frame) {
        let area = f.area();

        // Create a small info box in the top-center
        let info_width = 50u16.min(area.width - 4);
        let info_height = 8u16.min(area.height / 3);

        let info_rect = Rect {
            x: (area.width.saturating_sub(info_width)) / 2,
            y: 1,
            width: info_width,
            height: info_height,
        };

        let mut lines = vec![];

        // Current touch info
        if let Some(ref touch) = self.current_touch {
            lines.push(Line::from(vec![
                "Touch: ".bold(),
                format!("({},{}) ", touch.x, touch.y).green(),
            ]));
        } else {
            lines.push(Line::from("Touch the screen...".gray()));
        }

        lines.push(Line::from(""));

        // Statistics
        lines.push(Line::from(vec![
            "Samples: ".into(),
            format!("{}  ", self.statistics.total_samples).yellow(),
            "Jumps: ".into(),
            format!("{} ", self.statistics.total_jumps).red(),
        ]));

        // Controls
        lines.push(Line::from(vec![
            "R".bold().yellow(),
            ":Reset ".into(),
            "C".bold().yellow(),
            ":Clear ".into(),
            "T".bold().yellow(),
            ":Recalibrate ".into(),
            "Q".bold().yellow(),
            ":Quit".into(),
        ]));

        let info_widget = Paragraph::new(lines)
            .block(Block::bordered().title("Touch Test"))
            .style(Style::default().bg(Color::Black).fg(Color::White));

        f.render_widget(info_widget, info_rect);
    }

    fn draw_high_precision_canvas(&self, frame: &mut Frame, area: Rect) {
        // Use the ENTIRE area - no borders, no centering
        // This ensures the canvas size matches where you actually touch
        let canvas_w = area.width;
        let canvas_h = area.height;

        let mut canvas = vec![vec![' '; canvas_w as usize]; canvas_h as usize];

        // Draw corner markers to show calibrated area
        // Top-left
        if canvas_w > 2 && canvas_h > 2 {
            canvas[0][0] = '┌';
            canvas[0][1] = '─';
            canvas[1][0] = '│';

            // Top-right
            let tr_x = (canvas_w - 1) as usize;
            canvas[0][tr_x] = '┐';
            canvas[0][tr_x - 1] = '─';
            canvas[1][tr_x] = '│';

            // Bottom-left
            let br_y = (canvas_h - 1) as usize;
            canvas[br_y][0] = '└';
            canvas[br_y][1] = '─';
            canvas[br_y - 1][0] = '│';

            // Bottom-right
            canvas[br_y][tr_x] = '┘';
            canvas[br_y][tr_x - 1] = '─';
            canvas[br_y - 1][tr_x] = '│';
        }

        // Draw trail with fading
        let trail_len = self.trail.len();
        for (i, point) in self.trail.iter().enumerate() {
            let x = ((point.x as f32 / CALIBRATED_MAX_X as f32 * (canvas_w - 1) as f32) as usize)
                .min(canvas_w as usize - 1);
            let y = ((point.y as f32 / CALIBRATED_MAX_Y as f32 * (canvas_h - 1) as f32) as usize)
                .min(canvas_h as usize - 1);

            if x < canvas_w as usize && y < canvas_h as usize {
                // Fade trail: older points use lighter characters
                let age_ratio = i as f32 / trail_len as f32;
                let ch = if age_ratio > 0.8 {
                    'O' // Recent
                } else if age_ratio > 0.5 {
                    'o'
                } else {
                    '.' // Old
                };
                canvas[y][x] = ch;
            }
        }

        // Draw current touch with crosshair
        if let Some(ref touch) = self.current_touch {
            let cx = ((touch.x as f32 / CALIBRATED_MAX_X as f32 * (canvas_w - 1) as f32) as i32)
                .min(canvas_w as i32 - 1)
                .max(0);
            let cy = ((touch.y as f32 / CALIBRATED_MAX_Y as f32 * (canvas_h - 1) as f32) as i32)
                .min(canvas_h as i32 - 1)
                .max(0);

            // Ensure center point is within bounds
            if cx >= 0 && cx < canvas_w as i32 && cy >= 0 && cy < canvas_h as i32 {
                // Draw crosshair
                let size = 3i32;
                for dx in -size..=size {
                    let x = cx + dx;
                    if x >= 0 && x < canvas_w as i32 && cy >= 0 && cy < canvas_h as i32 {
                        canvas[cy as usize][x as usize] = '─';
                    }
                }
                for dy in -size..=size {
                    let y = cy + dy;
                    if y >= 0 && y < canvas_h as i32 && cx >= 0 && cx < canvas_w as i32 {
                        canvas[y as usize][cx as usize] = '│';
                    }
                }
                // Center marker
                canvas[cy as usize][cx as usize] = '┼';
            }
        }

        // Convert canvas to string
        let canvas_text: String = canvas
            .iter()
            .map(|row| row.iter().collect::<String>())
            .collect::<Vec<String>>()
            .join("\n");

        let style = Style::default().bg(Color::Black).fg(Color::White);

        // No border - use full area so touch position matches visual position
        let canvas_widget = Paragraph::new(canvas_text).style(style);

        frame.render_widget(canvas_widget, area);
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

                // Handle device selection screen
                if self.calibration.step == CalibrationStep::DeviceSelection {
                    match code {
                        KeyCode::KEY_UP => {
                            if !self.calibration.available_devices.is_empty() {
                                self.calibration.selected_device_index =
                                    (self.calibration.selected_device_index
                                        + self.calibration.available_devices.len()
                                        - 1)
                                        % self.calibration.available_devices.len();
                            }
                        }
                        KeyCode::KEY_DOWN => {
                            if !self.calibration.available_devices.is_empty() {
                                self.calibration.selected_device_index =
                                    (self.calibration.selected_device_index + 1)
                                        % self.calibration.available_devices.len();
                            }
                        }
                        KeyCode::KEY_ENTER | KeyCode::KEY_KPENTER => {
                            if !self.calibration.available_devices.is_empty() {
                                // Select the device and move to calibration
                                let selected = &self.calibration.available_devices
                                    [self.calibration.selected_device_index];
                                self.calibration.selected_device_path = Some(selected.path.clone());
                                self.calibration.selected_device_info = Some(selected.clone());
                                self.calibration.step = CalibrationStep::TopLeft;
                            }
                        }
                        KeyCode::KEY_1
                        | KeyCode::KEY_2
                        | KeyCode::KEY_3
                        | KeyCode::KEY_4
                        | KeyCode::KEY_5
                        | KeyCode::KEY_6
                        | KeyCode::KEY_7
                        | KeyCode::KEY_8
                        | KeyCode::KEY_9 => {
                            // Quick select by number
                            let idx = match code {
                                KeyCode::KEY_1 => 0,
                                KeyCode::KEY_2 => 1,
                                KeyCode::KEY_3 => 2,
                                KeyCode::KEY_4 => 3,
                                KeyCode::KEY_5 => 4,
                                KeyCode::KEY_6 => 5,
                                KeyCode::KEY_7 => 6,
                                KeyCode::KEY_8 => 7,
                                KeyCode::KEY_9 => 8,
                                _ => return Nav::Stay,
                            };
                            if idx < self.calibration.available_devices.len() {
                                self.calibration.selected_device_index = idx;
                                let selected = &self.calibration.available_devices[idx];
                                self.calibration.selected_device_path = Some(selected.path.clone());
                                self.calibration.selected_device_info = Some(selected.clone());
                                self.calibration.step = CalibrationStep::TopLeft;
                            }
                        }
                        _ => {}
                    }
                } else if code == KeyCode::KEY_R && self.calibration.is_done() {
                    // Reset statistics
                    self.statistics.reset();
                } else if code == KeyCode::KEY_C && self.calibration.is_done() {
                    // Clear trail
                    self.trail.clear();
                } else if code == KeyCode::KEY_T {
                    // Recalibrate - reset calibration to start over
                    self.calibration = Calibration::new();
                    self.trail.clear();
                    self.statistics.reset();
                    self.current_touch = None;
                    self.last_position = None;
                }
            }
            AppEvent::Tick => {
                // Update calibration hold duration on each tick
                if !self.calibration.is_done() {
                    self.calibration.update_hold_duration();
                } else {
                    // Remove old trail points based on time
                    let current_time = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis();

                    while let Some(front) = self.trail.front() {
                        if current_time.saturating_sub(front.timestamp) > TRAIL_LIFETIME_MS {
                            self.trail.pop_front();
                        } else {
                            break;
                        }
                    }
                }
            }
            _ => {}
        }

        Nav::Stay
    }
}
