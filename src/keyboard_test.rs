use evdev::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};
use std::collections::HashMap;

use crate::{
    Nav, Screen, ScreenId,
    event_handler::AppEvent,
    keyboard_layouts::{KeyboardLayout, LAYOUT_OPTIONS},
    machine_detect::get_computer_model,
};

const COLOR_LIST: [Color; 5] = [
    Color::Green,
    Color::Yellow,
    Color::Red,
    Color::Blue,
    Color::Magenta,
];

enum KeyboardTestMode {
    SelectLayout { selected: usize },
    Testing,
}

pub struct KeyboardTestScreen {
    ctrl_presses: usize,
    pressed_keys: HashMap<KeyCode, usize>,
    last_key_press: Option<AppEvent>,
    keyboard_layout: KeyboardLayout,
    mode: KeyboardTestMode,
}

impl KeyboardTestScreen {
    pub fn new() -> Self {
        let suggested_index = LAYOUT_OPTIONS
            .iter()
            .position(|option| {
                if let Some(model) = option.2 {
                    model == get_computer_model()
                } else {
                    false
                }
            })
            .unwrap_or(0);

        KeyboardTestScreen {
            ctrl_presses: 0,
            pressed_keys: HashMap::new(),
            last_key_press: None,
            keyboard_layout: LAYOUT_OPTIONS[suggested_index].1,
            mode: KeyboardTestMode::SelectLayout {
                selected: suggested_index,
            },
        }
    }
}

impl Screen for KeyboardTestScreen {
    fn id(&self) -> ScreenId {
        ScreenId::KeyboardTest
    }

    fn draw(&self, frame: &mut Frame) {
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(frame.area());

        match self.mode {
            KeyboardTestMode::SelectLayout { selected } => {
                self.draw_layout_header(frame, chunks[0]);
                self.draw_layout_list(frame, chunks[1], selected);
                self.draw_select_footer(frame, chunks[2]);
            }
            KeyboardTestMode::Testing => {
                self.draw_header(frame, chunks[0]);
                self.draw_keyboard(frame, chunks[1]);
                self.draw_footer(frame, chunks[2]);
            }
        }
    }

    fn handle_event(&mut self, event: AppEvent) -> Nav {
        match &mut self.mode {
            KeyboardTestMode::SelectLayout { selected } => {
                match event {
                    AppEvent::Key { code, .. } => match code {
                        KeyCode::KEY_DOWN => {
                            *selected = (*selected + 1) % LAYOUT_OPTIONS.len();
                        }
                        KeyCode::KEY_UP => {
                            *selected =
                                (*selected + LAYOUT_OPTIONS.len() - 1) % LAYOUT_OPTIONS.len();
                        }
                        KeyCode::KEY_ENTER => {
                            // Lock in the chosen layout and start the test
                            self.keyboard_layout = LAYOUT_OPTIONS[*selected].1;
                            self.pressed_keys.clear();
                            self.last_key_press = None;
                            self.ctrl_presses = 0;
                            self.mode = KeyboardTestMode::Testing;
                        }
                        KeyCode::KEY_ESC | KeyCode::KEY_Q => {
                            return Nav::To(ScreenId::Home);
                        }
                        // Still allow Ctrl×4 escape while on selection screen
                        KeyCode::KEY_LEFTCTRL | KeyCode::KEY_RIGHTCTRL => {
                            self.ctrl_presses += 1;
                            if self.ctrl_presses >= 4 {
                                return Nav::To(ScreenId::Home);
                            }
                        }
                        _ => {
                            // Any other key resets the Ctrl counter
                            self.ctrl_presses = 0;
                        }
                    },
                    _ => {}
                }
                return Nav::Stay;
            }

            KeyboardTestMode::Testing => {
                match event {
                    AppEvent::Key { code, .. } => {
                        if code == KeyCode::KEY_LEFTCTRL || code == KeyCode::KEY_RIGHTCTRL {
                            self.ctrl_presses += 1;
                        } else {
                            self.ctrl_presses = 0;
                        }

                        if self.ctrl_presses >= 4 {
                            return Nav::To(ScreenId::Home);
                        }

                        *self.pressed_keys.entry(code).or_insert(0) += 1;
                        self.last_key_press = Some(event);
                    }
                    _ => {}
                }
                return Nav::Stay;
            }
        }
    }
}

impl KeyboardTestScreen {
    fn draw_layout_header(&self, frame: &mut Frame, area: Rect) {
        let title = Line::from(vec![
            "Keyboard Test".bold().cyan(),
            " | ".into(),
            "Select keyboard layout".gray(),
        ]);
        let p = Paragraph::new(title).block(Block::bordered());
        frame.render_widget(p, area);
    }

    fn draw_layout_list(&self, frame: &mut Frame, area: Rect, selected: usize) {
        // Render a simple list with the selected entry highlighted
        let mut lines: Vec<Line> = Vec::with_capacity(LAYOUT_OPTIONS.len());
        for (i, (name, ..)) in LAYOUT_OPTIONS.iter().enumerate() {
            let marker = if i == selected { "› " } else { "  " };
            let line = if i == selected {
                Line::from(vec![Span::raw(marker), Span::raw(*name).bold().yellow()])
            } else {
                Line::from(vec![Span::raw(marker), Span::raw(*name)])
            };
            lines.push(line);
        }

        let p = Paragraph::new(lines)
            .block(Block::bordered().title("Available layouts"))
            .scroll((0, 0));
        frame.render_widget(p, area);
    }

    fn draw_select_footer(&self, frame: &mut Frame, area: Rect) {
        let help = Line::from(vec![
            "Use ".into(),
            "↑/↓".bold(),
            " to select • ".into(),
            "Enter".bold(),
            " to start test • ".into(),
            "Ctrl x4".bold(),
            " or ".into(),
            "Q/Esc".bold(),
            " to go back".into(),
        ])
        .centered();

        let p = Paragraph::new(help);
        frame.render_widget(p, area);
    }

    fn draw_header(&self, frame: &mut Frame, area: Rect) {
        let last_pressed = match &self.last_key_press {
            Some(AppEvent::Key { code, info }) => {
                format!("Last pressed: {:?} from {}", code, info.name)
            }
            _ => "Last pressed: (none)".to_string(),
        };

        let title = Line::from(vec![
            "Keyboard Test".bold().cyan(),
            " | ".into(),
            last_pressed.gray(),
        ]);

        let p = Paragraph::new(title).block(Block::bordered());

        frame.render_widget(p, area);
    }

    fn draw_keyboard(&self, frame: &mut Frame, area: Rect) {
        let vertical_chunks =
            Layout::vertical(self.keyboard_layout.iter().map(|_| Constraint::Fill(1))).split(area);

        self.draw_key_grid(frame, vertical_chunks[0], self.keyboard_layout[0][0]);

        if vertical_chunks.len() < 2 {
            return;
        }

        let horizontal_constraints = self.keyboard_layout[1].iter().map(|_| Constraint::Min(0));

        let horizontal_chunks = Layout::horizontal(horizontal_constraints)
            .spacing(2)
            .split(vertical_chunks[1]);

        self.draw_key_grid(frame, horizontal_chunks[0], self.keyboard_layout[1][0]);
        self.draw_key_grid(frame, horizontal_chunks[1], self.keyboard_layout[1][1]);
    }

    fn draw_key_grid(&self, frame: &mut Frame, area: Rect, keys: &[&[(&str, &[KeyCode])]]) {
        let key_height = 3;
        let row_spacing = 0;
        let column_spacing = 0;

        let row_constraints = keys.iter().map(|_| Constraint::Length(key_height));

        let vchunks = Layout::vertical(row_constraints)
            .spacing(row_spacing)
            .split(area);

        for (i, row) in keys.iter().enumerate() {
            let row_area = vchunks[i];

            let h_constraints = row
                .iter()
                .map(|(label, _)| Constraint::Min(label.len() as u16 + 2));

            let hchunks = Layout::horizontal(h_constraints)
                .spacing(column_spacing)
                .split(row_area);

            for (i, (label, keycodes)) in row.iter().enumerate() {
                let key_rect = hchunks[i];

                // Check if any of the keycodes for this button have been pressed
                let press_count = keycodes
                    .iter()
                    .map(|kc| self.pressed_keys.get(kc).unwrap_or(&0))
                    .max()
                    .unwrap_or(&0);

                self.draw_key(frame, key_rect, label, press_count);
            }
        }
    }

    fn draw_key(&self, frame: &mut Frame, area: Rect, label: &str, press_count: &usize) {
        let key_style = if *press_count == 0 {
            Style::default()
        } else {
            Style::default()
                .bg(COLOR_LIST[(press_count - 1) % 5])
                .black()
        };

        let block = Block::bordered().style(key_style);

        frame.render_widget(block, area);

        let key_label = Line::from(label);

        let text_position = Rect {
            x: area.x + (area.width.saturating_sub(label.len() as u16)) / 2,
            y: area.y + (area.height / 2),
            width: label.len() as u16,
            height: 1,
        };

        let p = Paragraph::new(key_label).centered();

        frame.render_widget(p, text_position);
    }

    fn draw_footer(&self, frame: &mut Frame, area: Rect) {
        let help = Line::from(vec![
            "Press CTRL ".into(),
            format!("{}", 4 - self.ctrl_presses).yellow().bold(),
            " times in a row to quit".into(),
        ])
        .centered();

        let p = Paragraph::new(help);
        frame.render_widget(p, area);
    }
}
