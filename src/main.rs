use std::env::args;

mod cli;
mod utils;

fn main() {
    let args = args().skip(1).collect::<String>();
    println!("Cmd : {}", args);

    if let Err(error) = cli::main(&args) {
        eprintln!("{}", error);
    };
}
