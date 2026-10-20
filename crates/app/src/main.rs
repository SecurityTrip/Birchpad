//! Birchpad desktop application: the program. The application itself is the library
//! (`src/lib.rs`).

// A GUI program: no console window on Windows (debug builds keep it for diagnostics).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    birchpad::run();
}
