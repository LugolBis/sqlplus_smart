use super::render::line_prompt;
use crossterm::{
    cursor::{MoveDown, MoveLeft, MoveRight, MoveTo, MoveToColumn, MoveUp},
    event::{KeyCode, KeyEvent},
    execute,
    style::ResetColor,
    terminal::{Clear, ClearType, disable_raw_mode},
};
use pty::fork::Master;
use std::io::{Write, stdout};

pub const HISTORY_MAX: usize = 100;

pub struct EditorState {
    pub lines: Vec<String>,
    pub current_line: usize,
    pub cursor_pos: usize,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub rendered_line_count: usize,
}

impl EditorState {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            current_line: 0,
            cursor_pos: 0,
            history: Vec::new(),
            history_index: None,
            rendered_line_count: 1,
        }
    }

    pub fn reset_input(&mut self) {
        self.lines = vec![String::new()];
        self.current_line = 0;
        self.cursor_pos = 0;
        self.history_index = None;
    }

    pub fn line(&self) -> &str {
        &self.lines[self.current_line]
    }

    pub fn is_last_line(&self) -> bool {
        self.current_line == self.lines.len() - 1
    }

    pub fn prompt_len(&self) -> usize {
        line_prompt(self.current_line + 1).len()
    }

    pub fn add_to_history(&mut self) {
        let entry = self.lines.join("\n");
        if !entry.trim().is_empty()
            && self.history.last().map(String::as_str) != Some(entry.as_str())
        {
            self.history.push(entry);
            if self.history.len() > HISTORY_MAX {
                self.history.remove(0);
            }
        }
    }
}

pub struct RestoreTerminal;

impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            stdout(),
            ResetColor,
            MoveToColumn(0),
            Clear(ClearType::CurrentLine)
        );
        println!();
    }
}
