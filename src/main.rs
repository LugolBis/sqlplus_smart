use std::env::args;

mod cli;
mod utils;

fn main() {
    let cmd = args().skip(1).collect::<Vec<String>>().join(" ");
    println!("Cmd : {}", cmd);

    if let Err(error) = cli::main(&cmd) {
        eprintln!("{}", error);
    };
}
