// Evita a janela de console extra em builds de release no Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    orbe_app_lib::run()
}
