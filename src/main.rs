//! 3D Chess: a board with pieces from OBJ models, drawn by OpenGL 4 shaders, and a computer to play against.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod camera;
mod chess;
mod gfx;
mod math;
mod mesh;
mod obj;
mod platform;
mod renderer;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() {
    if let Err(error) = app::run() {
        platform::show_error("3D Chess", &error.to_string());
        std::process::exit(1);
    }
}
