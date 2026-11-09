#![windows_subsystem = "windows"]

//! Tiny one-shot restorer used by the Windows logon entry.
//!
//! Deliberately links only the engine (no GUI framework) so it stays a few
//! hundred kilobytes and exits in milliseconds.

fn main() {
    lancer::apply_saved();
}
