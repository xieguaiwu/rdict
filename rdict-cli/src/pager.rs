use ansi_to_tui::IntoText;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    DefaultTerminal, Frame,
    buffer::Buffer,
    layout::Rect,
    style::Stylize,
    text::{Line, Text},
    widgets::{Block, Paragraph, Widget},
};
use std::io;

#[derive(Debug, Default)]
pub struct Pager {
    pub vertical_scroll: usize,
    pub exit: bool,
    pub text: String,
}

impl Pager {
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
    }

    /// updates the application's state based on user input
    fn handle_events(&mut self) -> io::Result<()> {
        match event::read()? {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.handle_key_event(key_event);
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) {
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), KeyModifiers::NONE) => self.exit(),

            (KeyCode::Char('j'), KeyModifiers::NONE)
            | (KeyCode::Down, KeyModifiers::NONE)
            | (KeyCode::Char('n'), KeyModifiers::CONTROL)
                if self.vertical_scroll + 1 < self.text.lines().count() =>
            {
                self.vertical_scroll += 1;
            }

            (KeyCode::Char('k'), KeyModifiers::NONE)
            | (KeyCode::Up, KeyModifiers::NONE)
            | (KeyCode::Char('p'), KeyModifiers::CONTROL)
                if self.vertical_scroll > 0 =>
            {
                self.vertical_scroll -= 1;
            }

            _ => {}
        }
    }

    const fn exit(&mut self) {
        self.exit = true;
    }
}

impl Widget for &Pager {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let instructions = Line::from("[q] to quit, [j/k] to scroll up and down".on_blue().black());
        let block = Block::new().title_bottom(instructions);
        let text: Text = self.text.clone().into_bytes().into_text()
            .expect("ansi_to_tui conversion failed");

        Paragraph::new(text)
            .scroll((self.vertical_scroll as u16, 0))
            .block(block)
            .render(area, buf);
    }
}
