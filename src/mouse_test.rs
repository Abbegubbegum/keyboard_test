use evdev::KeyCode;
use ratatui::{
    Frame,
    style::{Style, Stylize},
    symbols::border,
    text::Line,
    widgets::{Block, Paragraph},
};

use crate::{Nav, Screen, ScreenId, event_handler::AppEvent};

pub struct MouseTestScreen {
    cursor_x: f32,
    cursor_y: f32,
    sensitivity: f32,
    event_count: u64,
}

impl MouseTestScreen {
    pub fn new() -> Self {
        MouseTestScreen {
            cursor_x: 70.0, // Start in middle of typical terminal
            cursor_y: 20.0,
            sensitivity: 0.2,
            event_count: 0,
        }
    }
}

impl Screen for MouseTestScreen {
    fn id(&self) -> ScreenId {
        ScreenId::MouseTest
    }

    fn draw(&self, frame: &mut Frame) {
        let area = frame.area();

        let title = Line::from(vec![
            " Mouse Test ".bold().cyan(),
            format!("| Position: ({:.0}, {:.0}) ", self.cursor_x, self.cursor_y).into(),
            format!("| Events: {} ", self.event_count).yellow(),
        ]);
        let footer = Line::from(vec![
            " ↑/↓".bold().yellow(),
            " sensitivity   ".into(),
            "Space".bold().yellow(),
            " reset   ".into(),
            "Q/Esc".bold().yellow(),
            " exit   ".into(),
            format!("Sensitivity: {:.1}x ", self.sensitivity).yellow(),
        ]);

        let block = Block::bordered()
            .title(title.centered())
            .title_bottom(footer.centered())
            .border_set(border::THICK);

        frame.render_widget(block, area);

        // Draw cursor at the current position
        // Clamp cursor to be within terminal bounds
        let cursor_x = (self.cursor_x.round() as u16)
            .max(0)
            .min(area.width.saturating_sub(1));
        let cursor_y = (self.cursor_y.round() as u16)
            .max(0)
            .min(area.height.saturating_sub(1));

        // Create a simple cursor symbol
        let cursor = Paragraph::new("X").style(Style::default().bold().yellow());

        // Calculate the area for the cursor
        let cursor_area = ratatui::layout::Rect {
            x: cursor_x,
            y: cursor_y,
            width: 1,
            height: 1,
        };

        frame.render_widget(cursor, cursor_area);
    }

    fn handle_event(&mut self, event: AppEvent) -> Nav {
        match event {
            AppEvent::Key { code, .. } => match code {
                KeyCode::KEY_ESC | KeyCode::KEY_Q => {
                    return Nav::To(ScreenId::Home);
                }
                KeyCode::KEY_UP => {
                    // Increase sensitivity, max 5.0x
                    self.sensitivity = (self.sensitivity + 0.1).min(5.0);
                }
                KeyCode::KEY_DOWN => {
                    // Decrease sensitivity, min 0.1x
                    self.sensitivity = (self.sensitivity - 0.1).max(0.1);
                }
                KeyCode::KEY_SPACE => {
                    // Reset cursor position
                    self.cursor_x = 70.0;
                    self.cursor_y = 20.0;
                }
                _ => {}
            },
            AppEvent::Mouse { x, y, .. } => {
                // Update cursor position based on relative mouse movement with sensitivity
                // x and y are deltas, not absolute positions
                self.cursor_x += x as f32 * self.sensitivity;
                self.cursor_y += y as f32 * self.sensitivity;

                self.cursor_x = self.cursor_x.clamp(0.0, 200.0);
                self.cursor_y = self.cursor_y.clamp(0.0, 100.0);
                self.event_count += 1;
            }
            _ => {}
        }

        Nav::Stay
    }
}
