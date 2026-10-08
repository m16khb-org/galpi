// Release builds on Windows are GUI apps; without this a console window opens.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    galpi_lib::run();
}
