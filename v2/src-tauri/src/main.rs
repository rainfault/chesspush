#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    chesspush_lib::desktop::run();
}
