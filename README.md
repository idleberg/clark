# clark

A Rust adaptation of [clack](https://github.com/bombshell-dev/clack) — the same prompts, verified
against the JavaScript original rather than merely modelled on it.

- [`clark`](./crates/clark) — the prompts, ready to use in a terminal program.
- [`clark-core`](./crates/clark-core) — the state machines and Ratatui widgets behind them, with no
  I/O of their own.
- [`clark-cli`](./crates/clark-cli) — the same prompts for shell scripts. In the making.

Design notes and the reasoning behind the port live in [`docs/adr`](./docs/adr).
