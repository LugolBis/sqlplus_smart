use crate::utils::{RestoreTerminal, handle_key_event, redraw};
use crossterm::{
    event::{self, Event},
    terminal::enable_raw_mode,
};
use pty::fork::{Fork, Master};
use std::io::BufRead;
use std::io::{Write, stdout};
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub fn main(cmd: &str) -> Result<(), Box<dyn std::error::Error>> {
    let fork = Fork::from_ptmx().unwrap();

    let master: Option<Master> = fork.is_parent().ok();

    if let Some(master) = master {
        master_main(master)?;
    } else {
        child_main(cmd)?;
    }

    Ok(())
}

fn master_main(mut master: Master) -> Result<(), Box<dyn std::error::Error>> {
    // 2. Canal pour envoyer les lignes de sortie du processus enfant
    let (tx, rx) = mpsc::channel();

    // 3. Thread dédié à la lecture de la sortie du processus enfant
    thread::spawn(move || {
        let mut reader = std::io::BufReader::new(master);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break, // Fin de flux
                Ok(_) => {
                    if tx.send(line.clone()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // 4. Préparer le terminal pour la saisie avancée
    enable_raw_mode()?;
    let mut stdout = stdout();

    // Assurer la restauration du terminal même en cas de panique
    let _guard = RestoreTerminal;

    // État de la ligne de saisie
    let mut input = String::new();
    let mut cursor_pos = 0;
    let mut history: Vec<String> = Vec::new();
    let mut history_index: Option<usize> = None;

    // Afficher le premier prompt
    redraw(&mut stdout, &input, cursor_pos)?;

    // Boucle principale
    loop {
        // Poll pour un événement clavier avec timeout (100 ms)
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key_event) = event::read()? {
                let running = !handle_key_event(
                    key_event,
                    &mut input,
                    &mut cursor_pos,
                    &mut history,
                    &mut history_index,
                    &mut master,
                    &mut stdout,
                )?;

                if !running {
                    return Ok(());
                }
            }
        } else {
            // Pas de clé, on regarde si le processus enfant a produit des lignes
            if let Ok(line) = rx.try_recv() {
                // Afficher la ligne enfant sur stdout
                print!("{}", line);
                stdout.flush()?;
                // Redessiner la ligne de saisie
                redraw(&mut stdout, &input, cursor_pos)?;
            }
        }
    }
}

fn child_main(
    cmd: &str,
    /*
    mut child_w: PipeWriter,
    child_r: PipeReader,*/
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd_child = Command::new(cmd)
        //.stdin(child_r)
        //.stderr(child_w.try_clone()?)
        //.stdout(child_w.try_clone()?)
        .spawn()?;

    let _ = cmd_child.wait();
    // child_w.write(&"EXIT".as_bytes());
    Ok(())
}
