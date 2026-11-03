#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
pub mod error;
pub mod models;
pub mod services;

fn main() {
    app::run();
}
