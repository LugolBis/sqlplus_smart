use crate::utils::{RestoreTerminal, handle_key_event, redraw};
use crossterm::{
    event::{self, Event},
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
    // Dupliquer le fd : un pour lire (thread), un pour écrire (main).
    // Sans ça, le Mutex est tenu pendant le read() bloquant → deadlock garanti.
    let master_fd = master.as_raw_fd();
    let reader_fd = unsafe { libc::dup(master_fd) };
    if reader_fd < 0 {
        return Err(format!("dup() failed: {}", std::io::Error::last_os_error()).into());
    }

    // master_write : garde le Master original (implémente Write)
    let mut master_write = master;

    // master_read  : File construit sur le fd dupliqué (implémente Read)
    let mut master_read = unsafe { File::from_raw_fd(reader_fd) };

    let (tx, rx) = mpsc::channel::<Vec<u8>>();

    // Thread de lecture — utilise uniquement master_read, jamais de lock partagé
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

    let mut input = String::new();
    let mut cursor_pos = 0usize;
    let mut history: Vec<String> = Vec::new();
    let mut history_index: Option<usize> = None;

    redraw(&mut stdout, &input, cursor_pos)?;

    loop {
        // Événements clavier — timeout court pour ne pas bloquer la sortie enfant
        if event::poll(Duration::from_millis(10))? {
            if let Event::Key(key_event) = event::read()? {
                let should_quit = handle_key_event(
                    key_event,
                    &mut input,
                    &mut cursor_pos,
                    &mut history,
                    &mut history_index,
                    &mut master_write, // accès direct, sans Mutex
                    &mut stdout,
                )?;

                if should_quit {
                    return Ok(());
                }
            }
        }

        // Afficher ce que le processus enfant a produit
        while let Ok(chunk) = rx.try_recv() {
            stdout.write_all(&chunk)?;
            stdout.flush()?;
        }
    }
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
