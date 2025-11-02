#![warn(clippy::all, rust_2018_idioms)]

pub mod app;
pub mod assets;
pub mod config;
pub mod connection_manager;
pub mod db;
pub mod drag;
pub mod module;
pub mod save_load;
pub use app::App;
pub mod simulator;
