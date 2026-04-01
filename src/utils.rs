use crossterm::{
    cursor::{MoveDown, MoveLeft, MoveRight, MoveToColumn, MoveUp},
    event::{KeyCode, KeyEvent},
    execute,
    terminal::{Clear, ClearType, disable_raw_mode},
};
use pty::fork::Master;
use std::io::{Write, stdout};

const PROMPT: &str = "SQL> ";
const HISTORY_MAX: usize = 100;

// ── Prompt dynamique ────────────────────────────────────────────────────────
// Ligne 1  → "SQL> "
// Ligne N  → "  N> " (même largeur que PROMPT)
fn line_prompt(n: usize) -> String {
    if n == 1 {
        PROMPT.to_string()
    } else {
        let s = n.to_string();
        let pad = PROMPT.len().saturating_sub(s.len() + 2);
        format!("{}{}>  ", " ".repeat(pad), s)
    }
}

// ── État de l'éditeur ───────────────────────────────────────────────────────

pub struct EditorState {
    /// Toutes les lignes du buffer courant (toujours au moins 1).
    pub lines: Vec<String>,
    /// Indice de la ligne en cours d'édition.
    pub current_line: usize,
    /// Position du curseur dans la ligne courante (offset en chars).
    pub cursor_pos: usize,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    /// Nombre de lignes actuellement dessinées sur le terminal.
    /// Permet d'effacer les lignes résiduelles après une fusion.
    rendered_line_count: usize,
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

    /// Réinitialise le buffer d'entrée (pas l'historique).
    /// `rendered_line_count` est conservé pour que le prochain redraw_all
    /// efface correctement les lignes résiduelles.
    fn reset_input(&mut self) {
        self.lines = vec![String::new()];
        self.current_line = 0;
        self.cursor_pos = 0;
        self.history_index = None;
    }

    fn line(&self) -> &str {
        &self.lines[self.current_line]
    }

    fn is_last_line(&self) -> bool {
        self.current_line == self.lines.len() - 1
    }

    fn prompt_len(&self) -> usize {
        line_prompt(self.current_line + 1).len()
    }

    fn add_to_history(&mut self) {
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

// ── Fonctions de rendu ──────────────────────────────────────────────────────

/// Redessine l'intégralité du buffer depuis la position actuelle du curseur.
/// Suppose que le curseur est déjà au bon endroit (début du bloc ou position
/// quelconque après une sortie du processus enfant).
/// Utilisé après réception de données du processus enfant.
pub fn redraw_fresh(stdout: &mut std::io::Stdout, state: &mut EditorState) -> std::io::Result<()> {
    for (i, line) in state.lines.iter().enumerate() {
        execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
        write!(stdout, "{}{}", line_prompt(i + 1), line)?;
        if i < state.lines.len() - 1 {
            writeln!(stdout)?;
        }
    }
    // Positionner le curseur sur la ligne d'édition courante
    let rows_up = state.lines.len() - 1 - state.current_line;
    if rows_up > 0 {
        execute!(stdout, MoveUp(rows_up as u16))?;
    }
    let col = (state.prompt_len() + state.cursor_pos) as u16;
    execute!(stdout, MoveToColumn(col))?;
    state.rendered_line_count = state.lines.len();
    stdout.flush()
}

/// Redessine tout le bloc en remontant d'abord au début, puis en effaçant
/// les lignes résiduelles si le buffer s'est raccourci.
/// Utilisé pour toutes les opérations d'édition.
pub fn redraw_all(stdout: &mut std::io::Stdout, state: &mut EditorState) -> std::io::Result<()> {
    // Nombre total de lignes à couvrir (max entre rendu précédent et actuel)
    let total = state.rendered_line_count.max(state.lines.len());

    // 1. Remonter au début du bloc
    if state.current_line > 0 {
        execute!(stdout, MoveUp(state.current_line as u16))?;
    }

    // 2. Redessiner chaque ligne courante
    for (i, line) in state.lines.iter().enumerate() {
        execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
        write!(stdout, "{}{}", line_prompt(i + 1), line)?;
        if i < state.lines.len() - 1 {
            writeln!(stdout)?;
        }
    }

    // 3. Effacer les lignes résiduelles (fusion de lignes, Ctrl+C, etc.)
    let leftover = total.saturating_sub(state.lines.len());
    for _ in 0..leftover {
        writeln!(stdout)?;
        execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
    }

    // 4. Remonter à la ligne d'édition courante.
    //    Après les étapes 2+3, on est à la ligne (total - 1) du bloc (0-indexé).
    //    On veut être à current_line.
    let rows_up = total - 1 - state.current_line;
    if rows_up > 0 {
        execute!(stdout, MoveUp(rows_up as u16))?;
    }

    // 5. Positionner le curseur à la bonne colonne
    let col = (state.prompt_len() + state.cursor_pos) as u16;
    execute!(stdout, MoveToColumn(col))?;

    state.rendered_line_count = state.lines.len();
    stdout.flush()
}

// ── Gestionnaire d'événements clavier ───────────────────────────────────────

/// Retourne Ok(true) si on doit quitter, Ok(false) pour continuer.
pub fn handle_key_event(
    key_event: KeyEvent,
    state: &mut EditorState,
    master: &mut Master,
    stdout: &mut std::io::Stdout,
) -> std::io::Result<bool> {
    match key_event.code {
        KeyCode::Esc => {}

        // ── Caractères ────────────────────────────────────────────────────
        KeyCode::Char(c) => {
            if c == '\x03' {
                // Ctrl+C : annuler toute la saisie
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

        // ── Entrée ────────────────────────────────────────────────────────
        KeyCode::Enter => {
            // Commandes spéciales uniquement en mode ligne unique
            if state.lines.len() == 1 {
                let lower = state.line().trim().to_lowercase();
                if lower == "exit" {
                    master.write_all(b"exit\n")?;
                    return Ok(true);
                }
                if lower == "clear" {
                    execute!(stdout, MoveToColumn(0), Clear(ClearType::All))?;
                    state.reset_input();
                    state.rendered_line_count = 1;
                    redraw_fresh(stdout, state)?;
                    return Ok(false);
                }
            }

            if !state.is_last_line() {
                // Ligne intermédiaire : aller à la ligne suivante
                state.current_line += 1;
                state.cursor_pos = state.cursor_pos.min(state.line().len());
                let col = (state.prompt_len() + state.cursor_pos) as u16;
                execute!(stdout, MoveDown(1), MoveToColumn(col))?;
                stdout.flush()?;
            } else {
                // Dernière ligne : complétion ou continuation
                let last_trimmed = state.lines.last().unwrap().trim().to_string();
                let complete = last_trimmed.ends_with(';') || last_trimmed.ends_with('/');

                if complete {
                    state.add_to_history();
                    let command = state.lines.join("\n") + "\n";

                    // Descendre au bas du bloc avant le saut de ligne
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
                    // Insérer une nouvelle ligne vide après la courante
                    state.current_line += 1;
                    state.lines.insert(state.current_line, String::new());
                    state.cursor_pos = 0;
                    writeln!(stdout)?;
                    redraw_all(stdout, state)?;
                }
            }
        }

        // ── Backspace ─────────────────────────────────────────────────────
        KeyCode::Backspace => {
            if state.cursor_pos > 0 {
                // Supprimer le caractère avant le curseur
                state.lines[state.current_line].remove(state.cursor_pos - 1);
                state.cursor_pos -= 1;
                redraw_all(stdout, state)?;
            } else if state.current_line > 0 {
                // Début de ligne : fusionner avec la ligne précédente
                let current_content = state.lines.remove(state.current_line);
                state.current_line -= 1;
                let prev_len = state.lines[state.current_line].len();
                state.lines[state.current_line].push_str(&current_content);
                state.cursor_pos = prev_len;
                redraw_all(stdout, state)?;
            }
        }

        // ── Delete ────────────────────────────────────────────────────────
        KeyCode::Delete => {
            let line_len = state.lines[state.current_line].len();
            if state.cursor_pos < line_len {
                state.lines[state.current_line].remove(state.cursor_pos);
                redraw_all(stdout, state)?;
            } else if !state.is_last_line() {
                // Fin de ligne : fusionner la ligne suivante dans la courante
                let next = state.lines.remove(state.current_line + 1);
                state.lines[state.current_line].push_str(&next);
                redraw_all(stdout, state)?;
            }
        }

        // ── Flèche gauche ─────────────────────────────────────────────────
        KeyCode::Left => {
            if state.cursor_pos > 0 {
                state.cursor_pos -= 1;
                execute!(stdout, MoveLeft(1))?;
                stdout.flush()?;
            } else if state.current_line > 0 {
                // Début de ligne → fin de la ligne précédente
                state.current_line -= 1;
                state.cursor_pos = state.lines[state.current_line].len();
                let col = (line_prompt(state.current_line + 1).len() + state.cursor_pos) as u16;
                execute!(stdout, MoveUp(1), MoveToColumn(col))?;
                stdout.flush()?;
            }
        }

        // ── Flèche droite ─────────────────────────────────────────────────
        KeyCode::Right => {
            let line_len = state.lines[state.current_line].len();
            if state.cursor_pos < line_len {
                state.cursor_pos += 1;
                execute!(stdout, MoveRight(1))?;
                stdout.flush()?;
            } else if !state.is_last_line() {
                // Fin de ligne → début de la ligne suivante
                state.current_line += 1;
                state.cursor_pos = 0;
                let col = line_prompt(state.current_line + 1).len() as u16;
                execute!(stdout, MoveDown(1), MoveToColumn(col))?;
                stdout.flush()?;
            }
        }

        // ── Flèche haut ───────────────────────────────────────────────────
        KeyCode::Up => {
            if state.current_line > 0 {
                // Navigation dans le buffer multi-ligne
                state.current_line -= 1;
                state.cursor_pos = state.cursor_pos.min(state.lines[state.current_line].len());
                let col = (line_prompt(state.current_line + 1).len() + state.cursor_pos) as u16;
                execute!(stdout, MoveUp(1), MoveToColumn(col))?;
                stdout.flush()?;
            } else if state.lines.len() == 1 {
                // Historique (seulement en mode ligne unique)
                let new_index = match state.history_index {
                    Some(i) if i > 0 => i - 1,
                    None if !state.history.is_empty() => state.history.len() - 1,
                    _ => return Ok(false),
                };
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
        }

        // ── Flèche bas ────────────────────────────────────────────────────
        KeyCode::Down => {
            if state.current_line < state.lines.len() - 1 {
                // Navigation dans le buffer multi-ligne
                state.current_line += 1;
                state.cursor_pos = state.cursor_pos.min(state.lines[state.current_line].len());
                let col = (line_prompt(state.current_line + 1).len() + state.cursor_pos) as u16;
                execute!(stdout, MoveDown(1), MoveToColumn(col))?;
                stdout.flush()?;
            } else if state.lines.len() == 1 {
                // Historique (seulement en mode ligne unique)
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
                    None => {}
                }
            }
        }

        // ── Home / End ────────────────────────────────────────────────────
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

// ── Restauration du terminal ─────────────────────────────────────────────────

pub struct RestoreTerminal;

impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), MoveToColumn(0), Clear(ClearType::CurrentLine));
        println!();
    }
}
