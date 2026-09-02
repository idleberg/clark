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

mod autocomplete;
mod confirm;
mod date;
mod driver;
mod error;
mod group_multi_select;
pub mod keys;
mod message;
mod multi_line;
mod multi_select;
mod password;
mod path;
mod progress;
mod select;
mod select_key;
mod spinner;
mod task_log;
mod text;
mod ticker;

pub use autocomplete::{
	Autocomplete, AutocompleteMultiSelect, autocomplete, autocomplete_multiselect,
};
pub use confirm::{Confirm, confirm};
pub use date::{Date, date};
pub use error::ClackError;
pub use group_multi_select::{GroupMultiSelect, group_multiselect};
pub use message::{
	r#box, box_with, cancel, cancel_with, intro, intro_with, log, note, note_with, outro,
	outro_with,
};
pub use multi_line::{MultiLine, multiline};
pub use multi_select::{MultiSelect, multiselect};
pub use password::{Password, password};
pub use path::{Path, StdFs, path};
pub use progress::{Progress, progress};
pub use select::{Select, select};
pub use select_key::{SelectKey, select_key};
pub use spinner::{Spinner, spinner};
pub use task_log::{Group, TaskLog, task_log};
pub use text::{Text, text};

pub use clackatui_core::date::{Date as CivilDate, DateFormat};
pub use clackatui_core::multi_line::Focus;
pub use clackatui_core::progress::BarStyle;
/// The pieces a caller needs to configure a Prompt or to drive one themselves.
pub use clackatui_core::prompt::{Outcome, Validator};
pub use clackatui_core::select::SelectOption;
pub use clackatui_core::session::Session;
pub use clackatui_core::settings::{Action, DateMessages, Settings};
pub use clackatui_core::spinner::{Indicator, StyleFrame};
pub use clackatui_core::theme::Theme;
