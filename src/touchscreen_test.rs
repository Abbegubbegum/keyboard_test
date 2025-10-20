use std::cell;

use evdev::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Style, Stylize},
    symbols::border,
    text::Line,
    widgets::{Block, Paragraph},
};

use crate::{Nav, Screen, ScreenId, event_handler::AppEvent};

static MAX_X: u16 = 1024;
static MAX_Y: u16 = 768;

static COLS: u16 = 16;
static ROWS: u16 = 12;

pub struct TouchscreenTestScreen {
    counts: Vec<u32>,
    last_touch: Option<AppEvent>,
}

impl TouchscreenTestScreen {
    #[inline]
    fn idx(&self, c: usize, r: usize) -> usize {
        r * (COLS as usize) + c
    }

    pub fn new() -> Self {
        TouchscreenTestScreen {
            counts: vec![0; (COLS * ROWS) as usize],
            last_touch: None,
        }
    }

    fn mark(&mut self, x: u16, y: u16) {
        // X is inverted, going from right to left
        let col = COLS - 1 - (x * COLS / MAX_X);
        let row = y * ROWS / MAX_Y;

        let index = self.idx(col as usize, row as usize);
        if index < self.counts.len() {
            self.counts[index] = self.counts[index].saturating_add(1);
        }
    }

    fn coverage_ratio(&self) -> f32 {
        let touched = self.counts.iter().filter(|&&c| c > 0).count() as f32;
        touched / (self.counts.len() as f32)
    }

    fn reset(&mut self) {
        self.counts.fill(0);
    }

    fn draw_header(&self, frame: &mut Frame, area: Rect) {
        let last_touch_string = match &self.last_touch {
            Some(AppEvent::Touch { x, y, timestamp }) => {
                format!("Last touch: ({}, {}) at {}", x, y, timestamp)
            }
            _ => "No touch registered".to_string(),
        };

        let coverage_percent = (self.coverage_ratio() * 100.0).round() as u8;

        let title = Line::from(vec![
            "Touchscreen Test".bold().cyan(),
            " | ".into(),
            format!("Coverage: {}%", coverage_percent).gray(),
            " | ".into(),
            last_touch_string.gray(),
        ]);

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
                let hits = self.counts[self.idx(col as usize, row as usize)];

                let bg = if hits > 0 {
                    Color::Green
                } else {
                    Color::DarkGray
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
            AppEvent::Touch { x, y, .. } => {
                self.mark(x, y);
                self.last_touch = Some(event);
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
