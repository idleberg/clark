# clark-cli

![Crates.io License](https://img.shields.io/crates/l/clark-cli?style=for-the-badge)
[![Crates.io Version](https://img.shields.io/crates/v/clark-cli?style=for-the-badge)](https://crates.io/crates/clark-cli)
[![CI](https://img.shields.io/github/actions/workflow/status/idleberg/clark-cli/ci.yml?style=for-the-badge)](https://github.com/idleberg/clark-cli/actions)

CLI for clark, the Rust port of Bombshell's clack prompts 🦀

```sh
clark intro "Bleep"

album=$(clark select "Which album?" --option "Body Riddle" --option "Death Peak") || exit
clark confirm "Add the vinyl too?" && clark log --level success "Added one 2×LP."

clark outro "Enjoy the records."
```

The binary is called `clark`. One sub-command is one Prompt or one static renderer, named as clack
names it; its flags are named after clack's options, kebab-cased.

## Where the output goes

The answer goes to **stdout**, one value per line. The Prompt itself is drawn on **stderr**, so
`$(...)` captures the answer and none of the drawing. Static renderers — `intro`, `outro`,
`cancel`, `note`, `box`, `log` — are output rather than answers, and go to stdout.

## Exit codes

| Code  | Meaning                                                      |
| ----- | ------------------------------------------------------------ |
| `0`   | an answer, on stdout                                         |
| `1`   | `confirm` answered no                                        |
| `2`   | a failure — a bad flag, or a terminal that could not be read |
| `130` | the user cancelled, with `escape` or `ctrl+c`                |

`confirm` prints nothing: it is meant for `clark confirm "…" && something`.

## Prompts

| Sub-command                | Answer           | Flags                                                                                      |
| -------------------------- | ---------------- | ------------------------------------------------------------------------------------------ |
| `text`                     | one line         | `--placeholder` `--initial-value` `--default-value`                                        |
| `password`                 | one line         | `--mask` `--clear-on-error`                                                                |
| `confirm`                  | exit code        | `--active` `--inactive` `--initial-value` `--vertical`                                     |
| `select`                   | one value        | `--option` `--initial-value` `--max-items`                                                 |
| `multiselect`              | a value per line | `--option` `--initial-value` (repeatable) `--cursor-at` `--max-items` `--required`         |
| `group-multiselect`        | a value per line | `--group` `--initial-value` `--cursor-at` `--max-items` `--required` `--selectable-groups` |
| `select-key`               | one value        | `--option` `--initial-value` `--case-sensitive`                                            |
| `autocomplete`             | one value        | `--option` `--placeholder` `--initial-value` `--max-items`                                 |
| `autocomplete-multiselect` | a value per line | `--option` `--placeholder` `--initial-value` `--max-items` `--required`                    |
| `path`                     | one path         | `--root` `--initial-value` `--directory`                                                   |
| `date`                     | `YYYY-MM-DD`     | `--format` `--separator` `--initial-value` `--default-value` `--min-date` `--max-date`     |
| `multiline`                | several lines    | `--placeholder` `--initial-value` `--default-value` `--show-submit`                        |

Boolean flags take their value: `--required false`, `--vertical true`. Left out, each Prompt keeps
clack's own default. Every date is written `YYYY-MM-DD`, whatever `--format` draws.

### Options

Repeat `--option`, or pipe one per line — the two build the same list, so `ls` can write it:

```sh
ls ~/Music | clark select "Which album?" --max-items 10
```

Keys are read from `/dev/tty`, so a piped stdin does not stop a Prompt from working.

A group is one string, because the shell has no objects:

```sh
clark group-multiselect "Build a playlist" \
  --group "Body Riddle:Herr Bar,Ted,Herzog" \
  --group "Death Peak:Peak Magnetic,Hoova"
```

## Static renderers

```sh
clark intro "Bleep — order 4412"
clark log --level step "Clark — Body Riddle (2×LP)"   # message|info|success|step|warn|error
clark note "Warp Records, 2006" --title "Body Riddle"
clark box "Downloads stay in your account forever." --title Bleep
clark cancel "Basket abandoned."
clark outro "Thanks."
```

## Not here yet

`spinner`, `progress` and `task_log` are missing: each of them draws while something else runs, so
the CLI shape is `clark spinner -- <command>` rather than a flag, and that is a sub-process to
supervise rather than a Prompt to ask. Say the word and they can be added. So can option labels and
hints — an option is currently labelled by its own value.

[gum]: https://github.com/charmbracelet/gum
