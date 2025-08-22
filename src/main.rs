mod event_handler;

mod keyboard_test;

use color_eyre::Result;
use crossbeam_channel::unbounded;
use evdev::KeyCode;
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Style, Stylize},
    symbols::border,
    text::{Line, Text},
    widgets::{Block, HighlightSpacing, List, ListItem, ListState, Paragraph, StatefulWidget},
};

use crate::{
    event_handler::{AppEvent, spawn_device_listeners},
    keyboard_test::KeyboardTestScreen,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenId {
    Home,
    KeyboardTest,
    Exit,
}

pub trait Screen {
    fn id(&self) -> ScreenId;
    fn draw(&self, frame: &mut Frame);
    fn handle_event(&mut self, event: AppEvent) -> Nav {
        let _ = event;
        Nav::Stay
    }
}

pub enum Nav {
    Stay,
    To(ScreenId),
}

const MENU: &[(&str, ScreenId)] = &[
    ("Keyboard Test", ScreenId::KeyboardTest),
    ("Exit", ScreenId::Exit),
];

#[derive(Default)]
struct HomeScreen {
    selected: usize,
}

impl Screen for HomeScreen {
    fn id(&self) -> ScreenId {
        ScreenId::Home
    }

    fn draw(&self, frame: &mut Frame) {
        let area = frame.area();

        let title = Line::from("Input Diagnostics Tool".bold().cyan());

        let footer = Line::from(vec![
            "↑/↓".bold().yellow(),
            " navigate   ".into(),
            "Enter".bold().yellow(),
            " run   ".into(),
            "1..9".bold().yellow(),
            " quick launch   ".into(),
            "Esc".bold().yellow(),
            " exit".into(),
        ]);

        let block = Block::bordered()
            .title(title.centered())
            .title_bottom(footer.centered())
            .border_set(border::THICK);

        frame.render_widget(block, area);

        let vertical_center = Layout::vertical([Constraint::Length(MENU.len() as u16 * 3)])
            .flex(Flex::Center)
            .split(area)[0];

        let menu_rect = Layout::horizontal([Constraint::Percentage(25)])
            .flex(Flex::Center)
            .split(vertical_center)[0];

        let menu_items: Vec<ListItem> = MENU
            .iter()
            .enumerate()
            .map(|(i, (label, _))| {
                let style = if i == self.selected {
                    Style::default().black().on_yellow().bold()
                } else {
                    Style::default()
                };
                ListItem::new(
                    Text::from(vec![
                        "".into(),
                        Line::from(format!("{})  {label}", i + 1)),
                        "".into(),
                    ])
                    .centered(),
                )
                .style(style)
            })
            .collect();

        let menu = List::new(menu_items);

        frame.render_widget(menu, menu_rect);
    }

    fn handle_event(&mut self, event: AppEvent) -> Nav {
        match event {
            AppEvent::Key { code, .. } => match code {
                KeyCode::KEY_UP => {
                    self.selected = (self.selected + 1) % MENU.len();
                }
                KeyCode::KEY_DOWN => {
                    self.selected = (self.selected + MENU.len() - 1) % MENU.len();
                }
                KeyCode::KEY_ENTER => {
                    return Nav::To(MENU[self.selected].1);
                }
                KeyCode::KEY_ESC => return Nav::To(ScreenId::Exit),
                KeyCode::KEY_1 => return Nav::To(MENU[0].1),
                KeyCode::KEY_2 => return Nav::To(MENU[1].1),
                _ => {}
            },
            _ => {}
        }

        Nav::Stay
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let mut terminal = ratatui::init();

    let result = run(&mut terminal);

    ratatui::restore();

    return result;
}

fn run(terminal: &mut DefaultTerminal) -> Result<()> {
    let mut active_screen: Box<dyn Screen> = Box::new(HomeScreen::default());

    let (tx, rx) = unbounded();

    let mut exit = false;

    spawn_device_listeners(&tx)?;

    while !exit {
        terminal.draw(|f| active_screen.draw(f))?;

        let next_event = rx.recv()?;

        let navigation = active_screen.handle_event(next_event);

        match navigation {
            Nav::Stay => {}
            Nav::To(ScreenId::Exit) => {
                exit = true;
            }
            Nav::To(screen_id) => {
                active_screen = create_screen(screen_id);
            }
        }
    }

    Ok(())
}

fn create_screen(screen_id: ScreenId) -> Box<dyn Screen> {
    match screen_id {
        ScreenId::Home => Box::new(HomeScreen::default()),
        ScreenId::KeyboardTest => Box::new(KeyboardTestScreen::default()),
        ScreenId::Exit => {
            eprintln!("Cannot create Exit screen");
            Box::new(HomeScreen::default())
        }
    }
}
