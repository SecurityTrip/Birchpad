//! `birchpad-release`: see the library (`src/lib.rs`) for its commands.

use std::process::ExitCode;

use birchpad_release_tool::{Env, run};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args, &Env::from_process()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("birchpad-release: {error:#}");
            ExitCode::FAILURE
        }
    }
}
