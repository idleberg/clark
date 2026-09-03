# clark-core

![Crates.io License](https://img.shields.io/crates/l/clark-core?style=for-the-badge)
[![Crates.io Version](https://img.shields.io/crates/v/clark-core?style=for-the-badge)](https://crates.io/crates/clark-cli)
[![CI](https://img.shields.io/github/actions/workflow/status/idleberg/clark/ci.yml?style=for-the-badge)](https://github.com/idleberg/clark/actions)

The state machines and [Ratatui](https://ratatui.rs) widgets behind [`clark`](../clark), with no I/O
of their own. Use this crate if you want clack's prompts inside a terminal application you already
drive yourself; if you just want to ask a question at a prompt, use `clark`.

## Install

```sh
cargo add clark-core
```

## Use

A prompt is a `Prompt<S>` you feed keys into, plus a widget that draws its current state:

```rust
use clark_core::line_editor::{Key, KeyName};
use clark_core::prompt::{Outcome, Prompt};
use clark_core::text::{TextState, TextWidget};

let mut prompt = Prompt::new(TextState::new());

for c in "Clark".chars() {
    prompt.key(Some(&c.to_string()), &Key::named(KeyName::Char(c)));
}

// Draw it: `&TextWidget` is a Ratatui `Widget`, and `frame()` is the same
// content as styled text if you would rather lay it out yourself.
let widget = TextWidget::new(&prompt, "What is your name?");
let _frame = widget.frame();

prompt.key(Some("\r"), &Key::named(KeyName::Return));
assert!(matches!(prompt.outcome(), Some(Outcome::Submitted(_))));
```

Each module is one prompt — `text`, `confirm`, `select`, `multi_select`, `autocomplete`, `date`,
`path`, and the rest — alongside the pieces they share: `line_editor` (Node's `readline`, ported),
`width` (`fast-string-width`, ported), `wrap`, `frame`, `theme` and `emitter`.

Every one of those is checked against a recording of the JavaScript original rather than against a
hand-written expectation. The reasoning is in [`docs/adr`](../../docs/adr).

## License

This work is licensed under [The MIT License](LICENSE).
