// The library facade exposes only the embedded GUI session engine. Keep the
// complete Muxer implementation available to its binary without warning about
// code paths used only by the terminal app or its local server.
#![allow(dead_code)]

mod agent;
mod app;
mod bindings;
mod config;
mod control;
mod editor;
mod keys;
mod launch;
mod layout;
mod monitor;
mod pane;
mod persistence;
mod session;
mod terminal;
#[cfg(windows)]
mod windows;

pub mod gui;
