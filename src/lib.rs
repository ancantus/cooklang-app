//! Library crate holding the platform-independent application logic.
//!
//! Kept separate from the Dioxus binary so unit tests can run on the host
//! without the GUI/webview dependency tree (`cargo test --no-default-features`).

pub mod repository;
pub mod timers;
