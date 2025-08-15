mod event_handler;

use color_eyre::Result;
use crossbeam_channel::{Receiver, Sender, unbounded};
use evdev::KeyCode;
use ratatui::{
    DefaultTerminal, Frame,
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Style, Stylize},
    symbols::border,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Widget},
};
use std::io::{self, Write};
use std::{collections::HashMap, fmt::format};

use crate::event_handler::{AppEvent, DeviceInfo};

const KEYBOARD_LAYOUT: &[&[(&str, KeyCode)]] = &[
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

struct App {
    ctrl_presses: usize,
    pressed_keys: HashMap<KeyCode, usize>,

    event_receiver: Receiver<AppEvent>,
    event_sender: Sender<AppEvent>,

    last_key_press: Option<AppEvent>,
}

impl App {
    pub fn new() -> Self {
        let (tx, rx) = unbounded();

        App {
            ctrl_presses: 0,
            pressed_keys: HashMap::new(),
            event_receiver: rx,
            event_sender: tx,
            last_key_press: None,
        }
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        event_handler::spawn_device_listeners(&self.event_sender).unwrap();

        while self.ctrl_presses < 4 {
            terminal.draw(|f| self.draw(f))?;

            self.fetch_next_event();
        }

        Ok(())
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

    fn draw_header(&self, frame: &mut Frame, area: Rect) {
        let last_pressed = match &self.last_key_press {
            Some(AppEvent::Key { code, info }) => {
                format!("Last pressed: {:?} from {}", code, info.name)
            }
            _ => "Last pressed: (none)".to_string(),
        };

        let title = Line::from(vec![
            "Keyboard Test".bold(),
            " | ".into(),
            last_pressed.gray(),
        ]);

        let p = Paragraph::new(title).block(Block::bordered());

        frame.render_widget(p, area);
    }

    fn draw_keyboard(&self, frame: &mut Frame, area: Rect) {
        let key_height = 5;
        let horizontal_padding = 1;
        let vertical_padding = 1;

        let mut row_constraints = Vec::with_capacity(KEYBOARD_LAYOUT.len() * 2);

        for i in 0..KEYBOARD_LAYOUT.len() {
            row_constraints.push(Constraint::Length(key_height));
            if i != KEYBOARD_LAYOUT.len() - 1 {
                row_constraints.push(Constraint::Length(vertical_padding));
            }
        }

        let vchunks = Layout::vertical(row_constraints).split(area);

        for (i, row) in KEYBOARD_LAYOUT.iter().enumerate() {
            let row_area = vchunks[i * 2];

            let cols = row.len() as u32;

            let h_constraints: Vec<Constraint> =
                (0..cols).map(|_| Constraint::Ratio(1, cols + 2)).collect();

            let hchunks = Layout::horizontal(h_constraints)
                .flex(Flex::SpaceBetween)
                .split(row_area);

            for (i, (label, keycode)) in row.iter().enumerate() {
                let key_rect = hchunks[i];

                let press_count = self.pressed_keys.get(keycode).unwrap_or(&0);

                let colors = vec![
                    Color::LightGreen,
                    Color::LightYellow,
                    Color::LightRed,
                    Color::LightBlue,
                    Color::LightMagenta,
                ];

                let key_style = if *press_count == 0 {
                    Style::default().on_dark_gray().white()
                } else {
                    Style::default().bg(colors[(press_count - 1) % 5]).black()
                };

                let block = Block::bordered().style(key_style);

                frame.render_widget(block, key_rect);

                let key_label = Line::from(*label);

                let text_position = Layout::vertical([Constraint::Length(1)])
                    .flex(Flex::Center)
                    .split(key_rect);

                let p = Paragraph::new(key_label).centered();

                frame.render_widget(p, text_position[0]);
            }
        }
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

    fn fetch_next_event(&mut self) {
        match self.event_receiver.recv() {
            Ok(event) => {
                match event {
                    AppEvent::Key { code, .. } => {
                        if code == KeyCode::KEY_LEFTCTRL || code == KeyCode::KEY_RIGHTCTRL {
                            self.ctrl_presses += 1;
                        } else {
                            self.ctrl_presses = 0; // Reset if any other key is pressed
                        }

                        *self.pressed_keys.entry(code).or_insert(0) += 1;

                        self.last_key_press = Some(event);
                    }
                    _ => {}
                }
            }
            Err(e) => {
                eprintln!("Error receiving event: {}", e);
            }
        }
    }
}

fn print_keyboard(pressed_keys: &HashMap<KeyCode, usize>) {
    // ANSI color codes
    const RESET: &str = "\x1b[0m";

    const COLORS: &[&str] = &[
        "\x1b[42m\x1b[30m", // Green
        "\x1b[43m\x1b[30m", // Orange
        "\x1b[41m\x1b[30m", // Red
        "\x1b[44m\x1b[30m", // Blue
    ];

    // Clear screen
    print!("\x1b[2J\x1b[H");
    io::stdout().flush().unwrap();

    for row in KEYBOARD_LAYOUT {
        for &key in row.iter() {
            let (key_str, keycode) = key;

            let press_count = pressed_keys.get(&keycode).copied().unwrap_or(0);

            if press_count > 0 {
                print!(
                    "{}{:^6}{}",
                    COLORS.get((press_count - 1) % 4).copied().unwrap_or(""),
                    key_str,
                    RESET
                );
            } else {
                print!("{:^6}", key_str);
            }
        }
        println!();
    }
    println!();
    println!("Press CTRL 4 times in a row to exit.");
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let mut terminal = ratatui::init();

    let mut app = App::new();

    let result = app.run(&mut terminal);

    ratatui::restore();

    return result;
}
