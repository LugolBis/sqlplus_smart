use sqlplus_next::cli;
use std::env::args;

fn main() {
    let cmd = args().skip(1).collect::<Vec<String>>().join(" ");
    if let Err(error) = cli::main(&cmd) {
        eprintln!("{}", error);
    };
}
