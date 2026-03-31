use crossterm::{
    cursor::{MoveLeft, MoveToColumn},
    event::{KeyCode, KeyEvent},
    execute,
    terminal::{Clear, ClearType, disable_raw_mode},
};
use pty::fork::Master;
use std::io::{Write, stdout};

const PROMPT: &str = "SQL> ";
const HISTORY_MAX: usize = 100;

/// Gère les événements clavier
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
        KeyCode::Char(c) => {
            if c == '\x03' {
                // Ctrl+C : envoyer SIGINT au processus enfant
                let _ = master.write_all(&[3]);
            } else if c == '\x04' {
                // Ctrl+D : envoyer EOF
                let _ = master.write_all(&[4]);
            } else {
                // Caractère normal : insérer à la position du curseur
                input.insert(*cursor_pos, c);
                *cursor_pos += 1;
                redraw(stdout, input, *cursor_pos)?;
            }
        }
        KeyCode::Enter => {
            if input.trim().to_lowercase().starts_with("exit") {
                return Ok(true);
            }

            // Envoyer la commande au processus enfant
            let command = format!("{}\n", input);
            master.write_all(command.as_bytes())?;
            master.flush()?;

            // Ajouter à l'historique si non vide et différent du dernier
            if !input.is_empty() && history.last() != Some(input) {
                history.push(input.clone());
                if history.len() > HISTORY_MAX {
                    history.remove(0);
                }
            }

            // Réinitialiser la ligne de saisie
            input.clear();
            *cursor_pos = 0;
            *history_index = None;
            redraw(stdout, input, *cursor_pos)?;
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
                move_cursor_left(stdout, 1)?;
            }
        }
        KeyCode::Right => {
            if *cursor_pos < input.len() {
                *cursor_pos += 1;
                // Déplacer le curseur vers la droite d'une colonne
                execute!(stdout, MoveLeft(0))?; // astuce : MoveLeft(0) n'est pas standard, mieux utiliser MoveRight
                // Crossterm n'a pas de MoveRight, on peut utiliser MoveToColumn
                let col = (PROMPT.len() + *cursor_pos) as u16;
                execute!(stdout, MoveToColumn(col))?;
            }
        }
        KeyCode::Up => {
            // Navigation dans l'historique (précédent)
            let new_index = match history_index {
                Some(i) if *i > 0 => *i - 1,
                None if !history.is_empty() => history.len() - 1,
                _ => return Ok(false),
            };
            *history_index = Some(new_index);
            let hist_line = history[new_index].clone();
            *input = hist_line;
            *cursor_pos = input.len();
            redraw(stdout, input, *cursor_pos)?;
        }
        KeyCode::Down => {
            // Navigation dans l'historique (suivant)
            let new_index = match history_index {
                Some(i) if *i + 1 < history.len() => *i + 1,
                Some(_) => {
                    // On sort de l'historique
                    *history_index = None;
                    *input = String::new();
                    *cursor_pos = 0;
                    redraw(stdout, input, *cursor_pos)?;
                    return Ok(false);
                }
                None => return Ok(false),
            };
            *history_index = Some(new_index);
            let hist_line = history[new_index].clone();
            *input = hist_line;
            *cursor_pos = input.len();
            redraw(stdout, input, *cursor_pos)?;
        }
        KeyCode::Home => {
            *cursor_pos = 0;
            execute!(stdout, MoveToColumn(PROMPT.len() as u16))?;
        }
        KeyCode::End => {
            *cursor_pos = input.len();
            let col = (PROMPT.len() + input.len()) as u16;
            execute!(stdout, MoveToColumn(col))?;
        }
        _ => {}
    }
    Ok(false)
}

/// Redessine la ligne de saisie (efface la ligne et réécrit prompt + input)
pub fn redraw(stdout: &mut std::io::Stdout, input: &str, cursor_pos: usize) -> std::io::Result<()> {
    // Aller au début de la ligne courante
    execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
    // Afficher le prompt et la saisie
    print!("{}{}", PROMPT, input);
    stdout.flush()?;
    // Positionner le curseur
    let col = (PROMPT.len() + cursor_pos) as u16;
    execute!(stdout, MoveToColumn(col))?;
    Ok(())
}

/// Déplace le curseur vers la gauche d'un certain nombre de colonnes
fn move_cursor_left(stdout: &mut std::io::Stdout, count: u16) -> std::io::Result<()> {
    execute!(stdout, MoveLeft(count))?;
    Ok(())
}

/// Structure pour restaurer le terminal à la sortie
pub struct RestoreTerminal;

impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        // Rétablir l'affichage normal
        let _ = execute!(stdout(), MoveToColumn(0), Clear(ClearType::CurrentLine));
        println!(); // saut de ligne final
    }
}
