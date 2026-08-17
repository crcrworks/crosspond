#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![deny(unsafe_code)]

fn main() {
    crosspond_app_lib::run();
}
