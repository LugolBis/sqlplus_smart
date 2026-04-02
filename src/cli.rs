use crate::editor::{
    events::handle_key_event,
    render::{redraw_all, redraw_fresh},
    state::{EditorState, RestoreTerminal},
};
use crossterm::{
    event::{self, Event},
    execute,
    style::{Color, ResetColor, SetForegroundColor},
    terminal::enable_raw_mode,
};
use pty::fork::{Fork, Master};
use std::fs::File;
use std::io::{Read, Write, stdout};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub fn main(cmd: &str) -> Result<(), Box<dyn std::error::Error>> {
    let fork = Fork::from_ptmx()?;

    if let Some(master) = fork.is_parent().ok() {
        master_main(master)?;
    } else {
        child_main(cmd)?;
    }

    Ok(())
}

fn master_main(master: Master) -> Result<(), Box<dyn std::error::Error>> {
    let master_fd = master.as_raw_fd();
    let reader_fd = unsafe { libc::dup(master_fd) };
    if reader_fd < 0 {
        return Err(format!("dup() failed: {}", std::io::Error::last_os_error()).into());
    }

    let mut master_write = master;
    let mut master_read = unsafe { File::from_raw_fd(reader_fd) };

    let (tx, rx) = mpsc::channel::<Vec<u8>>();

    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match master_read.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    enable_raw_mode()?;
    let mut stdout = stdout();
    let _guard = RestoreTerminal;

    let mut state = EditorState::new();
    // Buffer pour reconstituer les lignes complètes avant colorisation
    let mut line_buf: Vec<u8> = Vec::new();

    redraw_all(&mut stdout, &mut state)?;

    loop {
        if event::poll(Duration::from_millis(10))? {
            if let Event::Key(key_event) = event::read()? {
                let should_quit =
                    handle_key_event(key_event, &mut state, &mut master_write, &mut stdout)?;
                if should_quit {
                    return Ok(());
                }
            }
        }

        // Drainer tout ce qui est disponible dans le canal
        let mut received = false;
        while let Ok(chunk) = rx.try_recv() {
            received = true;
            write_chunk_colorized(&mut stdout, &chunk, &mut line_buf)?;
        }

        if received {
            stdout.flush()?;
            redraw_fresh(&mut stdout, &mut state)?;
        }
    }
}

/// Traite un chunk brut de sortie du PTY en le découpant ligne par ligne.
///
/// Stratégie :
/// - On accumule les octets dans `line_buf` jusqu'à rencontrer `\n`.
/// - À chaque `\n`, on a une ligne complète : on la colorise si elle contient
///   un marqueur d'erreur Oracle, puis on l'écrit.
/// - Les octets restants (après le dernier `\n`) sont des données partielles —
///   on les écrit immédiatement tels quels (c'est typiquement le prompt
///   "SQL> " de sqlplus qui n'a pas de `\n`).
fn write_chunk_colorized(
    stdout: &mut std::io::Stdout,
    chunk: &[u8],
    line_buf: &mut Vec<u8>,
) -> std::io::Result<()> {
    for &byte in chunk {
        if byte == b'\n' {
            // Ligne complète : appliquer la colorisation
            write_line_colorized(stdout, line_buf)?;
            stdout.write_all(b"\n")?;
            line_buf.clear();
        } else {
            line_buf.push(byte);
        }
    }

    // Données partielles (ex: "SQL> ") : écrire immédiatement sans buffériser,
    // car on ne sait pas encore si un \n va arriver, et on ne veut pas bloquer
    // l'affichage du prompt.
    if !line_buf.is_empty() {
        stdout.write_all(line_buf)?;
        line_buf.clear();
    }

    Ok(())
}

/// Écrit une ligne (sans `\n`) en rouge si elle contient un marqueur d'erreur
/// Oracle. Les séquences ANSI éventuellement présentes sont préservées.
///
/// Marqueurs détectés :
///   - "ERROR"  → message générique sqlplus ("ERROR at line N:")
///   - "ORA-"   → code d'erreur Oracle ("ORA-00942: table or view does not exist")
///   - "SP2-"   → erreur SQL*Plus ("SP2-0310: unable to open file")
fn write_line_colorized(stdout: &mut std::io::Stdout, line: &[u8]) -> std::io::Result<()> {
    let text = String::from_utf8_lossy(line);

    let is_error = text.contains("ERROR") || text.contains("ORA-") || text.contains("SP2-");

    if is_error {
        execute!(stdout, SetForegroundColor(Color::Red))?;
        stdout.write_all(line)?;
        execute!(stdout, ResetColor)?;
    } else {
        stdout.write_all(line)?;
    }

    Ok(())
}

fn child_main(cmd: &str) -> Result<(), Box<dyn std::error::Error>> {
    use std::ffi::CString;

    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.is_empty() {
        return Err("Commande vide".into());
    }

    let args_c: Vec<CString> = parts
        .iter()
        .map(|s| CString::new(*s))
        .collect::<Result<_, _>>()?;

    let mut argv: Vec<*const libc::c_char> = args_c.iter().map(|s| s.as_ptr()).collect();
    argv.push(std::ptr::null());

    unsafe {
        libc::execvp(args_c[0].as_ptr(), argv.as_ptr());
    }

    Err(format!(
        "execvp failed for '{}': {}",
        parts[0],
        std::io::Error::last_os_error()
    )
    .into())
}
