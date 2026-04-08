use super::render::{line_prompt, redraw_all, redraw_fresh};
use super::state::EditorState;
use crossterm::{
    cursor::{MoveDown, MoveLeft, MoveRight, MoveTo, MoveToColumn, MoveUp},
    event::{KeyCode, KeyEvent},
    execute,
    terminal::{Clear, ClearType},
};
use pty::fork::Master;
use std::io::Write;

pub fn handle_key_event(
    key_event: KeyEvent,
    state: &mut EditorState,
    master: &mut Master,
    stdout: &mut std::io::Stdout,
) -> std::io::Result<bool> {
    match key_event.code {
        KeyCode::Esc => {}

        KeyCode::Char(c) => {
            if c == '\x03' {
                let _ = master.write_all(&[3]);
                state.reset_input();
                writeln!(stdout)?;
                redraw_all(stdout, state)?;
            } else if c == '\x04' {
                master.write_all(&[4])?;
            } else {
                state.lines[state.current_line].insert(state.cursor_pos, c);
                state.cursor_pos += 1;
                redraw_all(stdout, state)?;
            }
        }

        KeyCode::Enter => {
            if state.lines.len() == 1 {
                let lower = state.line().trim().to_lowercase();

                if lower == "exit" {
                    master.write_all(b"exit\n")?;
                    return Ok(true);
                }

                if lower == "clear" {
                    // Effacer tout le terminal et repositionner le curseur en (0, 0)
                    execute!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;
                    state.reset_input();
                    state.rendered_line_count = 1;
                    redraw_fresh(stdout, state)?;
                    return Ok(false);
                }
            }

            if !state.is_last_line() {
                state.current_line += 1;
                state.cursor_pos = state.cursor_pos.min(state.line().len());
                let col = (state.prompt_len() + state.cursor_pos) as u16;
                execute!(stdout, MoveDown(1), MoveToColumn(col))?;
                stdout.flush()?;
            } else {
                let last_trimmed = state.lines.last().unwrap().trim().to_string();
                let complete = last_trimmed.ends_with(';') || last_trimmed.ends_with('/');

                if complete {
                    state.add_to_history();
                    let command = state.lines.join("\n") + "\n";

                    let rows_down = state.lines.len() - 1 - state.current_line;
                    if rows_down > 0 {
                        execute!(stdout, MoveDown(rows_down as u16))?;
                    }
                    execute!(stdout, MoveToColumn(0))?;
                    writeln!(stdout)?;
                    stdout.flush()?;

                    state.reset_input();
                    state.rendered_line_count = 1;

                    master.write_all(command.as_bytes())?;
                    master.flush()?;
                } else {
                    state.current_line += 1;
                    state.lines.insert(state.current_line, String::new());
                    state.cursor_pos = 0;
                    writeln!(stdout)?;
                    redraw_all(stdout, state)?;
                }
            }
        }

        KeyCode::Backspace => {
            if state.cursor_pos > 0 {
                state.lines[state.current_line].remove(state.cursor_pos - 1);
                state.cursor_pos -= 1;
                redraw_all(stdout, state)?;
            } else if state.current_line > 0 {
                let current_content = state.lines.remove(state.current_line);
                state.current_line -= 1;
                let prev_len = state.lines[state.current_line].len();
                state.lines[state.current_line].push_str(&current_content);
                state.cursor_pos = prev_len;
                redraw_all(stdout, state)?;
            }
        }

        KeyCode::Delete => {
            let line_len = state.lines[state.current_line].len();
            if state.cursor_pos < line_len {
                state.lines[state.current_line].remove(state.cursor_pos);
                redraw_all(stdout, state)?;
            } else if !state.is_last_line() {
                let next = state.lines.remove(state.current_line + 1);
                state.lines[state.current_line].push_str(&next);
                redraw_all(stdout, state)?;
            }
        }

        KeyCode::Left => {
            if state.cursor_pos > 0 {
                state.cursor_pos -= 1;
                execute!(stdout, MoveLeft(1))?;
                stdout.flush()?;
            } else if state.current_line > 0 {
                state.current_line -= 1;
                state.cursor_pos = state.lines[state.current_line].len();
                let col = (line_prompt(state.current_line + 1).len() + state.cursor_pos) as u16;
                execute!(stdout, MoveUp(1), MoveToColumn(col))?;
                stdout.flush()?;
            }
        }

        KeyCode::Right => {
            let line_len = state.lines[state.current_line].len();
            if state.cursor_pos < line_len {
                state.cursor_pos += 1;
                execute!(stdout, MoveRight(1))?;
                stdout.flush()?;
            } else if !state.is_last_line() {
                state.current_line += 1;
                state.cursor_pos = 0;
                let col = line_prompt(state.current_line + 1).len() as u16;
                execute!(stdout, MoveDown(1), MoveToColumn(col))?;
                stdout.flush()?;
            }
        }

        KeyCode::Up => {
            if state.current_line > 0 {
                state.current_line -= 1;
                state.cursor_pos = state.cursor_pos.min(state.lines[state.current_line].len());
                let col = (line_prompt(state.current_line + 1).len() + state.cursor_pos) as u16;
                execute!(stdout, MoveUp(1), MoveToColumn(col))?;
                stdout.flush()?;
            } else {
                let new_index = match state.history_index {
                    Some(i) if i > 0 => i - 1,
                    Some(_) => return Ok(false),
                    None if !state.history.is_empty() => {
                        let prev_len = state.history.len();
                        state.add_to_history();
                        prev_len - 1
                    }
                    None => return Ok(false),
                };

                state.history_index = Some(new_index);
                let entry = state.history[new_index].clone();
                state.lines = entry.lines().map(String::from).collect();
                if state.lines.is_empty() {
                    state.lines = vec![String::new()];
                }
                state.current_line = 0;
                state.cursor_pos = state.lines[0].len();
                redraw_all(stdout, state)?;
            }
        }

        KeyCode::Down => {
            if state.current_line < state.lines.len() - 1 {
                state.current_line += 1;
                state.cursor_pos = state.cursor_pos.min(state.lines[state.current_line].len());
                let col = (line_prompt(state.current_line + 1).len() + state.cursor_pos) as u16;
                execute!(stdout, MoveDown(1), MoveToColumn(col))?;
                stdout.flush()?;
            } else {
                match state.history_index {
                    Some(i) if i + 1 < state.history.len() => {
                        let new_index = i + 1;
                        state.history_index = Some(new_index);
                        let entry = state.history[new_index].clone();
                        state.lines = entry.lines().map(String::from).collect();
                        if state.lines.is_empty() {
                            state.lines = vec![String::new()];
                        }
                        state.current_line = state.lines.len() - 1;
                        state.cursor_pos = state.lines[state.current_line].len();
                        redraw_all(stdout, state)?;
                    }
                    Some(_) => {
                        state.history_index = None;
                        state.lines = vec![String::new()];
                        state.current_line = 0;
                        state.cursor_pos = 0;
                        redraw_all(stdout, state)?;
                    }
                    None => {
                        if state.lines.len() > 1 {
                            state.add_to_history();
                            state.lines = vec![String::new()];
                            state.current_line = 0;
                            state.cursor_pos = 0;
                            redraw_all(stdout, state)?;
                        }
                    }
                }
            }
        }

        KeyCode::Home => {
            state.cursor_pos = 0;
            execute!(stdout, MoveToColumn(state.prompt_len() as u16))?;
            stdout.flush()?;
        }

        KeyCode::End => {
            state.cursor_pos = state.line().len();
            let col = (state.prompt_len() + state.cursor_pos) as u16;
            execute!(stdout, MoveToColumn(col))?;
            stdout.flush()?;
        }

        _ => {}
    }
    Ok(false)
}