#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(exit_code) = vanysound_app_lib::maybe_run_embedded_cli() {
        std::process::exit(exit_code);
    }

    vanysound_app_lib::run();
}
