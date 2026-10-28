//! A shared TUI/iced data workbench over one actor-owned model.
//!
//! The application owns 100,000 deterministic rows, a filter, and a
//! [`ferriso_workspace::TableState`]. Both frontends render only the visible
//! range; neither owns the data or selection.

pub mod app;
pub mod gui;
pub mod tui;
