use crossterm::{
    cursor::{MoveLeft, MoveRight, MoveToColumn, MoveToRow},
    event::{KeyCode, KeyEvent},
    execute,
    terminal::{Clear, ClearType, disable_raw_mode},
};
use pty::fork::Master;
use std::io::{Write, stdout};

const PROMPT: &str = "SQL> ";
const HISTORY_MAX: usize = 100;

/// Retourne le prompt à afficher selon le numéro de ligne (1-indexé).
/// Ligne 1 → "SQL> "
/// Ligne N → "  N> " (aligné sur la même largeur que PROMPT)
fn line_prompt(line_number: usize) -> String {
    if line_number == 1 {
        PROMPT.to_string()
    } else {
        // On cale sur la largeur de PROMPT (5 chars : "SQL> ")
        // ex: "  2> ", " 10> ", "100> "
        let num = line_number.to_string();
        let width = PROMPT.len(); // 5
        let pad = width.saturating_sub(num.len() + 2); // 2 pour "> "
        format!("{}{}>  ", " ".repeat(pad), num)
    }
}

/// Retourne Ok(true) si on doit quitter, Ok(false) pour continuer.
pub fn handle_key_event(
    key_event: KeyEvent,
    // Ligne en cours de saisie
    input: &mut String,
    cursor_pos: &mut usize,
    // Lignes déjà validées qui attendent d'être envoyées (mode multi-ligne)
    pending_lines: &mut Vec<String>,
    history: &mut Vec<String>,
    history_index: &mut Option<usize>,
    master: &mut Master,
    stdout: &mut std::io::Stdout,
) -> std::io::Result<bool> {
    // Numéro de la ligne courante (1-indexé)
    let current_line = pending_lines.len() + 1;

    match key_event.code {
        KeyCode::Esc => {
            return Ok(false);
        }

        KeyCode::Char(c) => {
            if c == '\x03' {
                // Ctrl+C : annuler la saisie en cours (y compris multi-ligne)
                pending_lines.clear();
                input.clear();
                *cursor_pos = 0;
                writeln!(stdout)?;
                stdout.flush()?;
                redraw(stdout, input, *cursor_pos, 1)?;
                return Ok(false);
            } else if c == '\x04' {
                let _ = master.write_all(&[4]);
            } else {
                input.insert(*cursor_pos, c);
                *cursor_pos += 1;
                redraw(stdout, input, *cursor_pos, current_line)?;
            }
        }

        KeyCode::Enter => {
            let trimmed = input.trim();

            // --- Commandes spéciales (seulement en première ligne, hors multi-ligne) ---
            if pending_lines.is_empty() {
                let lower = trimmed.to_lowercase();
                if lower == "exit" {
                    master.write_all(b"exit\n")?;
                    return Ok(true);
                }
                if lower == "clear" {
                    execute!(stdout, MoveToRow(0), Clear(ClearType::All))?;
                    input.clear();
                    *cursor_pos = 0;
                    redraw(stdout, input, *cursor_pos, 1)?;
                    return Ok(false);
                }
            }

            // --- Logique multi-ligne ---
            let is_complete = trimmed.ends_with(';') || trimmed.ends_with('/');

            if !is_complete && trimmed.is_empty() && pending_lines.is_empty() {
                // Ligne vide seule : on envoie juste un \n (comportement sqlplus standard)
                master.write_all(b"\n")?;
                master.flush()?;
                writeln!(stdout)?;
                stdout.flush()?;
                redraw(stdout, input, *cursor_pos, 1)?;
                return Ok(false);
            }

            // Ajouter la ligne courante aux lignes en attente
            pending_lines.push(input.clone());

            if is_complete {
                // Commande complète : assembler et envoyer d'un coup
                let full_command = pending_lines.join("\n") + "\n";

                // Historique (on stocke la commande complète)
                let hist_entry = pending_lines.join(" ");
                if !hist_entry.trim().is_empty()
                    && history.last().map(|s| s.as_str()) != Some(hist_entry.as_str())
                {
                    history.push(hist_entry);
                    if history.len() > HISTORY_MAX {
                        history.remove(0);
                    }
                }

                pending_lines.clear();

                master.write_all(full_command.as_bytes())?;
                master.flush()?;

                input.clear();
                *cursor_pos = 0;
                *history_index = None;

                execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
                writeln!(stdout)?;
                stdout.flush()?;
            } else {
                // Commande incomplète : passer à la ligne suivante
                let next_line = pending_lines.len() + 1;
                input.clear();
                *cursor_pos = 0;
                *history_index = None;

                // Afficher un saut de ligne puis le prompt de la prochaine ligne
                execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
                writeln!(stdout)?;
                stdout.flush()?;
                redraw(stdout, input, *cursor_pos, next_line)?;
            }
        }

        KeyCode::Backspace => {
            if *cursor_pos > 0 {
                input.remove(*cursor_pos - 1);
                *cursor_pos -= 1;
                redraw(stdout, input, *cursor_pos, current_line)?;
            } else if !pending_lines.is_empty() {
                // Backspace en début de ligne : remonter à la ligne précédente
                let prev = pending_lines.pop().unwrap();
                *input = prev;
                *cursor_pos = input.len();
                let prev_line = pending_lines.len() + 1;
                // Remonter d'une ligne visuellement
                execute!(
                    stdout,
                    crossterm::cursor::MoveUp(1),
                    MoveToColumn(0),
                    Clear(ClearType::CurrentLine)
                )?;
                redraw(stdout, input, *cursor_pos, prev_line)?;
            }
        }

        KeyCode::Delete => {
            if *cursor_pos < input.len() {
                input.remove(*cursor_pos);
                redraw(stdout, input, *cursor_pos, current_line)?;
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
                execute!(stdout, MoveRight(1))?;
            }
        }

        KeyCode::Up => {
            // Navigation historique uniquement en mode ligne simple
            if !pending_lines.is_empty() {
                return Ok(false);
            }
            let new_index = match history_index {
                Some(i) if *i > 0 => *i - 1,
                None if !history.is_empty() => history.len() - 1,
                _ => return Ok(false),
            };
            *history_index = Some(new_index);
            *input = history[new_index].clone();
            *cursor_pos = input.len();
            redraw(stdout, input, *cursor_pos, 1)?;
        }

        KeyCode::Down => {
            if !pending_lines.is_empty() {
                return Ok(false);
            }
            match history_index {
                Some(i) if *i + 1 < history.len() => {
                    let new_index = *i + 1;
                    *history_index = Some(new_index);
                    *input = history[new_index].clone();
                    *cursor_pos = input.len();
                    redraw(stdout, input, *cursor_pos, 1)?;
                }
                Some(_) => {
                    *history_index = None;
                    *input = String::new();
                    *cursor_pos = 0;
                    redraw(stdout, input, *cursor_pos, 1)?;
                }
                None => {}
            }
        }

        KeyCode::Home => {
            *cursor_pos = 0;
            let prompt = line_prompt(current_line);
            execute!(stdout, MoveToColumn(prompt.len() as u16))?;
        }

        KeyCode::End => {
            *cursor_pos = input.len();
            let prompt = line_prompt(current_line);
            execute!(stdout, MoveToColumn((prompt.len() + input.len()) as u16))?;
        }

        _ => {}
    }
    Ok(false)
}

/// Redessine la ligne de saisie courante.
/// `line_number` : 1 pour la première ligne (prompt "SQL> "), N pour les suivantes ("  N> ").
pub fn redraw(
    stdout: &mut std::io::Stdout,
    input: &str,
    cursor_pos: usize,
    line_number: usize,
) -> std::io::Result<()> {
    let prompt = line_prompt(line_number);
    execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
    write!(stdout, "{}{}", prompt, input)?;
    stdout.flush()?;
    execute!(stdout, MoveToColumn((prompt.len() + cursor_pos) as u16))?;
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
