//! State machines and `Widget` impls for clackatui. No I/O.
//!
//! See `tests/forced_width_probe.rs` for M0, the experiment the architecture rests on.

pub mod confirm;
pub mod cursor;
pub mod emitter;
pub mod frame;
pub mod group_multi_select;
pub mod limit_options;
pub mod line_editor;
pub mod multi_select;
pub mod password;
pub mod prompt;
pub mod select;
pub mod select_key;
pub mod session;
pub mod settings;
pub mod text;
pub mod theme;
pub mod width;
pub mod wrap;
