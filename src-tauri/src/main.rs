// Windows opens a console window behind a GUI app unless it is told not to.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tiinyapp_farm_launcher_lib::run()
}
