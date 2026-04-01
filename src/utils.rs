use crossterm::{
    cursor::{MoveLeft, MoveRight, MoveToColumn, MoveToRow, MoveUp},
    event::{KeyCode, KeyEvent},
    execute,
    terminal::{Clear, ClearType, disable_raw_mode},
};
use pty::fork::Master;
use std::io::{Write, stdout};

const PROMPT: &str = "SQL> ";
const HISTORY_MAX: usize = 100;

/// Retourne Ok(true) si on doit quitter, Ok(false) pour continuer.
pub fn handle_key_event(
    key_event: KeyEvent,
    input: &mut String,
    cursor_pos: &mut usize,
    history: &mut Vec<String>,
    history_index: &mut Option<usize>,
    master: &mut Master,
    stdout: &mut std::io::Stdout,
) -> std::io::Result<bool> {
    match key_event.code {
        KeyCode::Esc => {
            return Ok(false);
        }
        KeyCode::Char(c) => {
            // Ctrl+C (ETX)
            if c == '\x03' {
                let _ = master.write_all(&[3]);
                return Ok(false);
            // Ctrl+D (EOT)
            } else if c == '\x04' {
                let _ = master.write_all(&[4]);
            } else {
                input.insert(*cursor_pos, c);
                *cursor_pos += 1;
                redraw(stdout, input, *cursor_pos)?;
            }
        }
        KeyCode::Enter => {
            let cleaned_input = input.trim().to_lowercase();
            if cleaned_input == "exit" {
                master.write_all(format!("{}\n", input).as_bytes())?;
                return Ok(true); // quitter
            } else if cleaned_input == "clear" {
                execute!(stdout, MoveToRow(0), MoveUp(10), Clear(ClearType::All))?;
                input.clear();
                *cursor_pos = 0;
                redraw(stdout, input, *cursor_pos)?;
                return Ok(false);
            }

            let command = format!("{}\n", input);
            master.write_all(command.as_bytes())?;
            master.flush()?;

            if !input.is_empty() && history.last().map(|s| s.as_str()) != Some(input.as_str()) {
                history.push(input.clone());
                if history.len() > HISTORY_MAX {
                    history.remove(0);
                }
            }

            input.clear();
            *cursor_pos = 0;
            *history_index = None;
            // Aller à la ligne — sqlplus va afficher sa propre réponse
            execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
            writeln!(stdout)?;
            stdout.flush()?;
        }
        KeyCode::Backspace => {
            if *cursor_pos > 0 {
                input.remove(*cursor_pos - 1);
                *cursor_pos -= 1;
                redraw(stdout, input, *cursor_pos)?;
            }
        }
        KeyCode::Delete => {
            if *cursor_pos < input.len() {
                input.remove(*cursor_pos);
                redraw(stdout, input, *cursor_pos)?;
            }
        }
        KeyCode::Left => {
            if *cursor_pos > 0 {
                *cursor_pos -= 1;
                execute!(stdout, MoveLeft(1))?;
            }
        }
        KeyCode::Right => {
            if *cursor_pos < input.len() {
                *cursor_pos += 1;
                execute!(stdout, MoveRight(1))?; // MoveRight existe bien dans crossterm
            }
        }
        KeyCode::Up => {
            let new_index = match history_index {
                Some(i) if *i > 0 => *i - 1,
                None if !history.is_empty() => history.len() - 1,
                _ => return Ok(false),
            };
            *history_index = Some(new_index);
            *input = history[new_index].clone();
            *cursor_pos = input.len();
            redraw(stdout, input, *cursor_pos)?;
        }
        KeyCode::Down => match history_index {
            Some(i) if *i + 1 < history.len() => {
                let new_index = *i + 1;
                *history_index = Some(new_index);
                *input = history[new_index].clone();
                *cursor_pos = input.len();
                redraw(stdout, input, *cursor_pos)?;
            }
            Some(_) => {
                *history_index = None;
                *input = String::new();
                *cursor_pos = 0;
                redraw(stdout, input, *cursor_pos)?;
            }
            None => {}
        },
        KeyCode::Home => {
            *cursor_pos = 0;
            execute!(stdout, MoveToColumn(PROMPT.len() as u16))?;
        }
        KeyCode::End => {
            *cursor_pos = input.len();
            execute!(stdout, MoveToColumn((PROMPT.len() + input.len()) as u16))?;
        }
        _ => {}
    }
    Ok(false) // continuer
}

/// Redessine la ligne de saisie courante
pub fn redraw(stdout: &mut std::io::Stdout, input: &str, cursor_pos: usize) -> std::io::Result<()> {
    execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
    write!(stdout, "{}{}", PROMPT, input)?;
    stdout.flush()?;
    let col = (PROMPT.len() + cursor_pos) as u16;
    execute!(stdout, MoveToColumn(col))?;
    Ok(())
}

pub struct RestoreTerminal;

impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), MoveToColumn(0), Clear(ClearType::CurrentLine));
        println!();
    }
}
