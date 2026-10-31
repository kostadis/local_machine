use std::process::ExitCode;

use clap::Parser;

use local_machine::cli::Cli;
use local_machine::system::RealSystem;

fn main() -> ExitCode {
    let cli = Cli::parse();
    match local_machine::run(&cli, &RealSystem) {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(local_machine::exit_code(&err))
        }
    }
}
