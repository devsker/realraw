//! Top-level app modules.

#[allow(clippy::module_inception)]
mod app;
pub mod about_dialog;
pub mod central;
pub mod library;
pub mod menubar;
pub mod setup_dialog;
pub mod status_bar;
pub mod tasks_panel;

pub use app::App;
