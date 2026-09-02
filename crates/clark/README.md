# clark

Bombshell's [clack](https://github.com/bombshell-dev/clack) prompts ported to Rust 🦀

## Install

```sh
cargo add clark
```

## Use

```rust
use clark::{ClackError, select, text};

fn main() -> Result<(), ClackError> {
    clark::intro("New project");

    let name = text("What is your name?")
        .placeholder("Anonymous")
        .interact()?;

    let colour = select("Pick a colour")
        .option("red")
        .option("green")
        .initial_value("green")
        .interact()?;

    clark::outro(format!("Hello {name}, in {colour}."));
    Ok(())
}
```

Every prompt is a builder ending in `interact()`, which returns `Err(ClackError::Cancelled)` when
the user presses Ctrl+C or Escape, or `interact_opt()`, which returns `None` there instead.

Prompts: `text`, `password`, `multiline`, `confirm`, `select`, `select_key`, `multiselect`,
`group_multiselect`, `autocomplete`, `autocomplete_multiselect`, `date`, `path`.

Output: `intro`, `outro`, `cancel`, `log`, `note`, `box`, plus `spinner`, `progress` and `task_log`
for work that takes a while.

Runnable examples for all of them:

```sh
cargo run -p clark --example select
```

## Drawing it yourself

If you have your own Ratatui application and want the widgets without the driver, depend on
[`clark-core`](../clark-core) instead.

## License

This work is licensed under [The MIT License](LICENSE).
