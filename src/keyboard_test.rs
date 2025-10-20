use evdev::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Block, Paragraph},
};
use std::collections::HashMap;

use crate::{Nav, Screen, ScreenId, event_handler::AppEvent};

const COLOR_LIST: [Color; 5] = [
    Color::Green,
    Color::Yellow,
    Color::Red,
    Color::Blue,
    Color::Magenta,
];

const MAIN_KEYBOARD_LAYOUT: &[&[(&str, KeyCode)]] = &[
    &[
        ("ESC", KeyCode::KEY_ESC),
        ("F1", KeyCode::KEY_F1),
        ("F2", KeyCode::KEY_F2),
        ("F3", KeyCode::KEY_F3),
        ("F4", KeyCode::KEY_F4),
        ("F5", KeyCode::KEY_F5),
        ("F6", KeyCode::KEY_F6),
        ("F7", KeyCode::KEY_F7),
        ("F8", KeyCode::KEY_F8),
        ("F9", KeyCode::KEY_F9),
        ("F10", KeyCode::KEY_F10),
        ("F11", KeyCode::KEY_F11),
        ("F12", KeyCode::KEY_F12),
    ],
    &[
        ("`", KeyCode::KEY_GRAVE),
        ("1", KeyCode::KEY_1),
        ("2", KeyCode::KEY_2),
        ("3", KeyCode::KEY_3),
        ("4", KeyCode::KEY_4),
        ("5", KeyCode::KEY_5),
        ("6", KeyCode::KEY_6),
        ("7", KeyCode::KEY_7),
        ("8", KeyCode::KEY_8),
        ("9", KeyCode::KEY_9),
        ("0", KeyCode::KEY_0),
        ("+", KeyCode::KEY_MINUS),
        ("`", KeyCode::KEY_EQUAL),
        ("Backspace", KeyCode::KEY_BACKSPACE),
    ],
    &[
        ("Tab", KeyCode::KEY_TAB),
        ("Q", KeyCode::KEY_Q),
        ("W", KeyCode::KEY_W),
        ("E", KeyCode::KEY_E),
        ("R", KeyCode::KEY_R),
        ("T", KeyCode::KEY_T),
        ("Y", KeyCode::KEY_Y),
        ("U", KeyCode::KEY_U),
        ("I", KeyCode::KEY_I),
        ("O", KeyCode::KEY_O),
        ("P", KeyCode::KEY_P),
        ("Å", KeyCode::KEY_LEFTBRACE),
        ("^", KeyCode::KEY_RIGHTBRACE),
        ("Enter", KeyCode::KEY_ENTER),
    ],
    &[
        ("CapsLock", KeyCode::KEY_CAPSLOCK),
        ("A", KeyCode::KEY_A),
        ("S", KeyCode::KEY_S),
        ("D", KeyCode::KEY_D),
        ("F", KeyCode::KEY_F),
        ("G", KeyCode::KEY_G),
        ("H", KeyCode::KEY_H),
        ("J", KeyCode::KEY_J),
        ("K", KeyCode::KEY_K),
        ("L", KeyCode::KEY_L),
        ("Ö", KeyCode::KEY_SEMICOLON),
        ("Ä", KeyCode::KEY_APOSTROPHE),
        ("'", KeyCode::KEY_BACKSLASH),
    ],
    &[
        ("Shift", KeyCode::KEY_LEFTSHIFT),
        ("<", KeyCode::KEY_102ND),
        ("Z", KeyCode::KEY_Z),
        ("X", KeyCode::KEY_X),
        ("C", KeyCode::KEY_C),
        ("V", KeyCode::KEY_V),
        ("B", KeyCode::KEY_B),
        ("N", KeyCode::KEY_N),
        ("M", KeyCode::KEY_M),
        (",", KeyCode::KEY_COMMA),
        (".", KeyCode::KEY_DOT),
        ("-", KeyCode::KEY_SLASH),
        ("RShift", KeyCode::KEY_RIGHTSHIFT),
    ],
    &[
        ("LCtrl", KeyCode::KEY_LEFTCTRL),
        ("LWin", KeyCode::KEY_LEFTMETA),
        ("Alt", KeyCode::KEY_LEFTALT),
        ("Space", KeyCode::KEY_SPACE),
        ("Alt Gr", KeyCode::KEY_RIGHTALT),
        ("RWin", KeyCode::KEY_RIGHTMETA),
        ("RCtrl", KeyCode::KEY_RIGHTCTRL),
    ],
];

const SIDE_LAYOUT: &[&[(&str, KeyCode)]] = &[
    &[
        ("Insert", KeyCode::KEY_INSERT),
        ("Home", KeyCode::KEY_HOME),
        ("Page Up", KeyCode::KEY_PAGEUP),
    ],
    &[
        ("Delete", KeyCode::KEY_DELETE),
        ("End", KeyCode::KEY_END),
        ("Page Down", KeyCode::KEY_PAGEDOWN),
    ],
    &[],
    &[("↑", KeyCode::KEY_UP)],
    &[
        ("←", KeyCode::KEY_LEFT),
        ("↓", KeyCode::KEY_DOWN),
        ("→", KeyCode::KEY_RIGHT),
    ],
];

const NUMPAD_LAYOUT: &[&[(&str, KeyCode)]] = &[
    &[
        ("Num Lock", KeyCode::KEY_NUMLOCK),
        ("/", KeyCode::KEY_KPSLASH),
        ("*", KeyCode::KEY_KPASTERISK),
        ("-", KeyCode::KEY_KPMINUS),
    ],
    &[
        ("7", KeyCode::KEY_KP7),
        ("8", KeyCode::KEY_KP8),
        ("9", KeyCode::KEY_KP9),
        ("+", KeyCode::KEY_KPPLUS),
    ],
    &[
        ("4", KeyCode::KEY_KP4),
        ("5", KeyCode::KEY_KP5),
        ("6", KeyCode::KEY_KP6),
    ],
    &[
        ("1", KeyCode::KEY_KP1),
        ("2", KeyCode::KEY_KP2),
        ("3", KeyCode::KEY_KP3),
    ],
    &[
        ("0", KeyCode::KEY_KP0),
        (".", KeyCode::KEY_KPDOT),
        ("Enter", KeyCode::KEY_KPENTER),
    ],
];

#[derive(Default)]
pub struct KeyboardTestScreen {
    ctrl_presses: usize,
    pressed_keys: HashMap<KeyCode, usize>,
    last_key_press: Option<AppEvent>,
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

        self.draw_header(frame, chunks[0]);

        self.draw_keyboard(frame, chunks[1]);

        self.draw_footer(frame, chunks[2]);
    }

    fn handle_event(&mut self, event: AppEvent) -> Nav {
        match event {
            AppEvent::Key { code, .. } => {
                if code == KeyCode::KEY_LEFTCTRL || code == KeyCode::KEY_RIGHTCTRL {
                    self.ctrl_presses += 1;
                } else {
                    self.ctrl_presses = 0; // Reset if any other key is pressed
                }

                if self.ctrl_presses >= 4 {
                    return Nav::To(ScreenId::Home);
                }

                *self.pressed_keys.entry(code).or_insert(0) += 1;

                self.last_key_press = Some(event);
            }
            _ => {}
        }

        Nav::Stay
    }
}

impl KeyboardTestScreen {
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
            Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);

        let horizontal_chunks = Layout::horizontal([
            Constraint::Percentage(30),
            Constraint::Percentage(40),
            Constraint::Percentage(30),
        ])
        .split(vertical_chunks[1]);

        self.draw_key_grid(frame, vertical_chunks[0], MAIN_KEYBOARD_LAYOUT);
        self.draw_key_grid(frame, horizontal_chunks[0], SIDE_LAYOUT);
        self.draw_key_grid(frame, horizontal_chunks[2], NUMPAD_LAYOUT);
    }

    fn draw_key_grid(&self, frame: &mut Frame, area: Rect, keys: &[&[(&str, KeyCode)]]) {
        let key_height = 3;
        let row_spacing = 1;
        let column_spacing = 1;

        let mut row_constraints = Vec::with_capacity(keys.len() * 2);

        for i in 0..keys.len() {
            row_constraints.push(Constraint::Length(key_height));
            if i != keys.len() - 1 {
                row_constraints.push(Constraint::Length(row_spacing));
            }
        }

        let vchunks = Layout::vertical(row_constraints).split(area);

        for (i, row) in keys.iter().enumerate() {
            let row_area = vchunks[i * 2];

            let cols = row.len();

            let mut h_constraints: Vec<Constraint> = Vec::with_capacity(cols * 2);

            for i in 0..cols {
                let (label, _) = row[i];
                h_constraints.push(Constraint::Min(label.len() as u16 + 2));
                if i != cols - 1 {
                    h_constraints.push(Constraint::Length(column_spacing));
                }
            }
            let hchunks = Layout::horizontal(h_constraints).split(row_area);

            for (i, (label, keycode)) in row.iter().enumerate() {
                let key_rect = hchunks[i * 2];

                let press_count = self.pressed_keys.get(keycode).unwrap_or(&0);

                self.draw_key(frame, key_rect, label, press_count);
            }
        }
    }

    fn draw_key(&self, frame: &mut Frame, area: Rect, label: &str, press_count: &usize) {
        let key_style = if *press_count == 0 {
            Style::default().on_dark_gray().white()
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
