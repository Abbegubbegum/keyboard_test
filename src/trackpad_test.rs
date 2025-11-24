use evdev::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Block, Paragraph},
};

use crate::{
    Nav, Screen, ScreenId,
    event_handler::{AppEvent, FingerState, TrackpadEvent},
};

const TRACKPAD_WIDTH: u16 = 80;
const TRACKPAD_HEIGHT: u16 = 40;
const MAX_SLOTS: usize = 10;

pub struct TrackpadTestScreen {
    // Track finger positions for each slot
    fingers: Vec<Option<FingerState>>,
    // Click state
    is_clicked: bool,
    // Finger count from BTN_TOOL events
    finger_count: Option<usize>,
    // Event counter for debugging
    event_count: u64,
}

impl TrackpadTestScreen {
    pub fn new() -> Self {
        Self {
            fingers: vec![None; MAX_SLOTS],
            is_clicked: false,
            finger_count: None,
            event_count: 0,
        }
    }

    fn draw_header(&self, frame: &mut Frame, area: Rect) {
        let mut text = vec![];

        text.push("Trackpad Test".bold().cyan());
        text.push(" | ".into());

        // Show finger count
        if let Some(count) = self.finger_count {
            text.push(format!("Fingers: {}", count).yellow());
        } else {
            text.push("Fingers: 0".gray());
        }

        text.push(" | ".into());

        // Show click state
        if self.is_clicked {
            text.push("CLICKED".bold().red());
        } else {
            text.push("Not clicked".gray());
        }

        text.push(" | ".into());

        // Show active finger slots
        let active_slots: Vec<usize> = self
            .fingers
            .iter()
            .enumerate()
            .filter_map(|(i, f)| if f.is_some() { Some(i) } else { None })
            .collect();

        if active_slots.is_empty() {
            text.push("No active touches".gray());
        } else {
            text.push(format!("Active slots: {:?}", active_slots).green());
        }

        let title = Line::from(text);
        let p = Paragraph::new(title).block(Block::bordered());
        frame.render_widget(p, area);
    }

    fn draw_footer(&self, frame: &mut Frame, area: Rect) {
        let footer = Line::from(vec![
            "Press ".into(),
            "Q/Esc".bold().yellow(),
            " to quit. Swipe, tap, and click on trackpad to test.".into(),
        ])
        .centered();

        frame.render_widget(footer, area);
    }

    fn draw_trackpad(&self, frame: &mut Frame, area: Rect) {
        // Center the trackpad representation
        let vertical_chunks = Layout::vertical([
            Constraint::Min(0),
            Constraint::Length(TRACKPAD_HEIGHT + 2), // +2 for border
            Constraint::Min(0),
        ])
        .split(area);

        let horizontal_chunks = Layout::horizontal([
            Constraint::Min(0),
            Constraint::Length(TRACKPAD_WIDTH + 2), // +2 for border
            Constraint::Min(0),
        ])
        .split(vertical_chunks[1]);

        let trackpad_area = horizontal_chunks[1];

        // Create a buffer to draw on
        let mut buffer = vec![vec![' '; TRACKPAD_WIDTH as usize]; TRACKPAD_HEIGHT as usize];

        // Draw finger positions
        for (slot, finger_state) in self.fingers.iter().enumerate() {
            if let Some(finger) = finger_state {
                if let (Some(x), Some(y)) = (finger.x, finger.y) {
                    // Normalize coordinates (assuming typical trackpad ranges)
                    // You may need to adjust these based on your actual trackpad ranges
                    let max_x = 1200; // Adjust based on your trackpad
                    let max_y = 800; // Adjust based on your trackpad

                    let norm_x = (x.max(0).min(max_x) as f32 / max_x as f32 * TRACKPAD_WIDTH as f32)
                        as usize;
                    let norm_y = (y.max(0).min(max_y) as f32 / max_y as f32
                        * TRACKPAD_HEIGHT as f32) as usize;

                    let norm_x = norm_x.min(TRACKPAD_WIDTH as usize - 1);
                    let norm_y = norm_y.min(TRACKPAD_HEIGHT as usize - 1);

                    // Draw finger marker with slot number
                    let marker = std::char::from_digit(slot as u32, 10).unwrap_or('*');
                    buffer[norm_y][norm_x] = marker;

                    // Draw a small cross around the finger for better visibility
                    if norm_x > 0 {
                        buffer[norm_y][norm_x - 1] = '-';
                    }
                    if norm_x < TRACKPAD_WIDTH as usize - 1 {
                        buffer[norm_y][norm_x + 1] = '-';
                    }
                    if norm_y > 0 {
                        buffer[norm_y - 1][norm_x] = '|';
                    }
                    if norm_y < TRACKPAD_HEIGHT as usize - 1 {
                        buffer[norm_y + 1][norm_x] = '|';
                    }
                }
            }
        }

        // Convert buffer to string
        let trackpad_text: String = buffer
            .iter()
            .map(|row| row.iter().collect::<String>())
            .collect::<Vec<String>>()
            .join("\n");

        // Choose background color based on click state
        let bg_color = if self.is_clicked {
            Color::DarkGray
        } else {
            Color::Black
        };

        let style = Style::default().bg(bg_color).fg(Color::White);

        let trackpad = Paragraph::new(trackpad_text)
            .style(style)
            .block(Block::bordered().title("Trackpad Surface"));

        frame.render_widget(trackpad, trackpad_area);
    }

    fn draw_info_panel(&self, frame: &mut Frame, area: Rect) {
        let mut lines = vec![];

        lines.push(Line::from("Finger Details:".bold().cyan()));
        lines.push(Line::from(""));

        let mut has_active = false;
        for (slot, finger_state) in self.fingers.iter().enumerate() {
            if let Some(finger) = finger_state {
                has_active = true;
                lines.push(Line::from(format!("Slot {}: ", slot).yellow()));

                if let Some(x) = finger.x {
                    lines.push(Line::from(format!("  X: {}", x)));
                }
                if let Some(y) = finger.y {
                    lines.push(Line::from(format!("  Y: {}", y)));
                }
                if let Some(pressure) = finger.pressure {
                    lines.push(Line::from(format!("  Pressure: {}", pressure)));
                }
                if let Some(major) = finger.major {
                    lines.push(Line::from(format!("  Touch Major: {}", major)));
                }
                if let Some(width) = finger.width {
                    lines.push(Line::from(format!("  Width: {}", width)));
                }
                lines.push(Line::from(""));
            }
        }

        if !has_active {
            lines.push(Line::from("No fingers detected".gray()));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(
            format!("Total events: {}", self.event_count).gray(),
        ));

        let info = Paragraph::new(lines).block(Block::bordered().title("Info"));

        frame.render_widget(info, area);
    }
}

impl Screen for TrackpadTestScreen {
    fn id(&self) -> ScreenId {
        ScreenId::TrackpadTest
    }

    fn draw(&self, frame: &mut Frame) {
        let main_chunks = Layout::vertical([
            Constraint::Length(3), // Header
            Constraint::Min(5),    // Main area
            Constraint::Length(1), // Footer
        ])
        .split(frame.area());

        self.draw_header(frame, main_chunks[0]);
        self.draw_footer(frame, main_chunks[2]);

        // Split main area into trackpad and info panel
        let content_chunks = Layout::horizontal([
            Constraint::Min(TRACKPAD_WIDTH + 4), // Trackpad area
            Constraint::Length(30),              // Info panel
        ])
        .split(main_chunks[1]);

        self.draw_trackpad(frame, content_chunks[0]);
        self.draw_info_panel(frame, content_chunks[1]);
    }

    fn handle_event(&mut self, event: AppEvent) -> Nav {
        match event {
            AppEvent::Key { code, .. } => {
                if code == KeyCode::KEY_Q || code == KeyCode::KEY_ESC {
                    return Nav::To(ScreenId::Home);
                }
            }
            AppEvent::Trackpad { event } => {
                self.event_count += 1;
                match event {
                    TrackpadEvent::FingerUpdate { slot, state } => {
                        if slot < MAX_SLOTS {
                            self.fingers[slot] = Some(state);
                        }
                    }
                    TrackpadEvent::FingerUp { slot } => {
                        if slot < MAX_SLOTS {
                            self.fingers[slot] = None;
                        }
                    }
                    TrackpadEvent::Click { down } => {
                        self.is_clicked = down;
                    }
                    TrackpadEvent::FingerCount { count } => {
                        self.finger_count = Some(count);
                    }
                }
            }
            _ => {}
        }

        Nav::Stay
    }
}
