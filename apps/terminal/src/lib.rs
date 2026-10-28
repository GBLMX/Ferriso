//! A minimal embedded terminal for the iced frontend.
//!
//! `rio-vt` owns the VT parser, grid, and PTY loop. This crate owns the
//! frontend-neutral snapshot and the iced cell renderer.
pub mod app;
pub mod gui;
mod layout;
