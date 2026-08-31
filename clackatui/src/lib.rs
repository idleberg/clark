//! clack's prompts, for Rust.
//!
//! ```no_run
//! let name = clackatui::text("What is your name?").interact()?;
//! # Ok::<_, clackatui::ClackError>(())
//! ```
//!
//! The state machines, the Frames and the Emitter are all in `clackatui-core`, which performs no
//! I/O and can be drawn into someone else's Ratatui application. This crate is the part that owns a
//! terminal: raw mode, a blocking read loop, and clack's sugar on top of it.
//!
//! # Where the compatibility claim lives
//!
//! Almost nowhere here. Every question with a right answer — how text is measured, how a line is
//! wrapped, what a keypress does to the field, which bytes a Frame change costs — is settled in
//! `clackatui-core` against a harvested recording of the real library. What this crate adds is the
//! terminal itself, and one thing that is a genuine port and not yet verified: turning crossterm's
//! key events into the ones Node's `readline` would have reported. See [`keys`] for what that
//! involves and what it is still owed.

mod confirm;
mod driver;
mod error;
pub mod keys;
mod password;
mod select;
mod text;

pub use confirm::{Confirm, confirm};
pub use error::ClackError;
pub use password::{Password, password};
pub use select::{Select, select};
pub use text::{Text, text};

/// The pieces a caller needs to configure a Prompt or to drive one themselves.
pub use clackatui_core::prompt::{Outcome, Validator};
pub use clackatui_core::select::SelectOption;
pub use clackatui_core::session::Session;
pub use clackatui_core::settings::{Action, Settings};
pub use clackatui_core::theme::Theme;
