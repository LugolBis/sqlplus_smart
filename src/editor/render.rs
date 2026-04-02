use super::state::EditorState;
use crossterm::{
    cursor::{MoveToColumn, MoveUp},
    execute,
    terminal::{Clear, ClearType},
};
use std::io::Write;

pub const PROMPT: &str = "SQL> ";

pub fn line_prompt(n: usize) -> String {
    if n == 1 {
        PROMPT.to_string()
    } else {
        let s = n.to_string();
        let pad = PROMPT.len().saturating_sub(s.len() + 2);
        format!("{}{}>  ", " ".repeat(pad), s)
    }
}

// ── Rendu ───────────────────────────────────────────────────────────────────

pub fn redraw_fresh(stdout: &mut std::io::Stdout, state: &mut EditorState) -> std::io::Result<()> {
    for (i, line) in state.lines.iter().enumerate() {
        execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
        write!(stdout, "{}{}", line_prompt(i + 1), line)?;
        if i < state.lines.len() - 1 {
            writeln!(stdout)?;
        }
    }
    let rows_up = state.lines.len() - 1 - state.current_line;
    if rows_up > 0 {
        execute!(stdout, MoveUp(rows_up as u16))?;
    }
    let col = (state.prompt_len() + state.cursor_pos) as u16;
    execute!(stdout, MoveToColumn(col))?;
    state.rendered_line_count = state.lines.len();
    stdout.flush()
}

pub fn redraw_all(stdout: &mut std::io::Stdout, state: &mut EditorState) -> std::io::Result<()> {
    let total = state.rendered_line_count.max(state.lines.len());

    if state.current_line > 0 {
        execute!(stdout, MoveUp(state.current_line as u16))?;
    }

    for (i, line) in state.lines.iter().enumerate() {
        execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
        write!(stdout, "{}{}", line_prompt(i + 1), line)?;
        if i < state.lines.len() - 1 {
            writeln!(stdout)?;
        }
    }

    let leftover = total.saturating_sub(state.lines.len());
    for _ in 0..leftover {
        writeln!(stdout)?;
        execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
    }

    let rows_up = total - 1 - state.current_line;
    if rows_up > 0 {
        execute!(stdout, MoveUp(rows_up as u16))?;
    }
    let col = (state.prompt_len() + state.cursor_pos) as u16;
    execute!(stdout, MoveToColumn(col))?;
    state.rendered_line_count = state.lines.len();
    stdout.flush()
}

// ── Gestionnaire d'événements clavier ───────────────────────────────────────
