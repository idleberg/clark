# clark-cli

![Crates.io License](https://img.shields.io/crates/l/clark-cli?style=for-the-badge)
[![Crates.io Version](https://img.shields.io/crates/v/clark-cli?style=for-the-badge)](https://crates.io/crates/clark-cli)
[![CI](https://img.shields.io/github/actions/workflow/status/idleberg/clark-cli/ci.yml?style=for-the-badge)](https://github.com/idleberg/clark-cli/actions)

CLI for clark, the Rust port of Bombshell's clack prompts 🦀

## Install

### Cargo

```
cargo install clark-cli
```

### Homebrew

```
brew install idleberg/asahi/clark
```

## Use

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
`cancel`, `note`, `box`, `log` — are output rather than answers, and go to stdout. `spinner` and
`task-log` redraw as they run, so they are drawn on stderr with the Prompts, leaving stdout to the
command they wrap.

## Exit codes

| Code  | Meaning                                                      |
| ----- | ------------------------------------------------------------ |
| `0`   | an answer, on stdout                                         |
| `1`   | `confirm` answered no                                        |
| `2`   | a failure — a bad flag, or a terminal that could not be read |
| `130` | the user cancelled, with `escape` or `ctrl+c`                |

`confirm` prints nothing: it is meant for `clark confirm "…" && something`.

`spinner` and `task-log` report the exit code of the command they wrap, whatever it is. A command
killed by a signal — which is what `ctrl+c` does to it — is reported as `130`.

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

A boolean flag on its own means `true` — `--vertical`. Several of clack's booleans are already
`true`, so they also take a value to turn off: `--required false`. Left out entirely, each Prompt
keeps clack's own default. Every date is written `YYYY-MM-DD`, whatever `--format` draws.

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

## Renderers that wrap a command

A Prompt ends when you answer it. A spinner has no answer to wait for — it runs for as long as the
work does, and from a shell that work is another process. So these two take the command instead,
after a `--`, and end when it does:

```sh
clark spinner "Ordering the vinyl" -- sleep 3
clark task-log "Ripping the CD" --limit 5 -- ffmpeg -i track.wav track.flac
```

| Sub-command | Draws                              | Flags                                                                                             |
| ----------- | ---------------------------------- | ------------------------------------------------------------------------------------------------- |
| `spinner`   | a turning symbol, and nothing else | `--indicator dots\|timer` `--frame` `--delay` `--stop-message` `--error-message` `--cancel-message` `--show-error` |
| `task-log`  | the command's output, dimmed       | `--limit` `--spacing` `--retain-log` `--stop-message` `--error-message` `--show-log`               |

**`spinner` shows nothing the command writes.** That is clack's own design and not a shortcut: every
frame walks the cursor back over the row it drew, so anything printed underneath is inside the
region the next frame erases. Its stdout is discarded; its stderr is kept back and printed under the
red row if it fails, which `--show-error false` turns off.

**`task-log` is the one that shows output.** It reads the command's stdout and stderr, keeps the
last `--limit` rows on screen, and then erases the whole log if the command succeeds — or leaves it
under a red line if it does not. Rows from the two streams are each in order, but the two are only
interleaved as they arrive.

Both report the command's own exit code, so `clark spinner "…" -- make && …` reads the way it looks.

## Not here yet

`progress` is missing. The other two end when the command does, which is something clark can see for
itself; a bar has to be told how far along it is, and there is nowhere for a shell to say so. Option
labels and hints are missing too — an option is currently labelled by its own value.

[gum]: https://github.com/charmbracelet/gum
