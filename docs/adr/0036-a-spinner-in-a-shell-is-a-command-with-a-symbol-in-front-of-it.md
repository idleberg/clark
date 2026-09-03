# A spinner in a shell is a command with a symbol in front of it

`clark-cli`'s twelve Prompts all have the same shape: draw on stderr, read a key, print the answer,
exit. What bounds each of them is the answer — the process ends because the user settled it.

The three renderers driven by calls rather than by keys have no answer to be bounded by. A
[`spinner`](./0032-a-spinner-walks-the-cursor-back-over-a-string-it-never-wrote.md) runs for as long
as the caller's work runs, and from a shell that work is a different process. `clark spinner "…"` on
its own would exit and take the animation with it. So the sub-command takes the work too:

```sh
clark spinner "Ordering the vinyl" -- sleep 3
```

clark forks, supervises, and ends the renderer on the child's exit. `spinner` and `task-log` are
both in [`crates/clark-cli/src/main.rs`](../../crates/clark-cli/src/main.rs) on that shape.

## The spinner shows nothing, because upstream's cannot

`gum spin` has `--show-output`, and the obvious thing is to copy it. clack has no counterpart —
`SpinnerOptions` is `indicator | onCancel | cancelMessage | errorMessage | frames | delay |
styleFrame` and nothing else. That is not an omission. Every tick calls `clearPrevMessage`, which
walks the cursor up over the rows it last drew and erases down from there; anything a child wrote in
between is inside the region the next frame erases. A spinner that showed output would be a spinner
that ate it.

So `clark spinner` discards the child's stdout. Its stderr is held back rather than dropped and
printed under the red row when the command fails, which `--show-error false` turns off — a silent
failed build is worse than a small divergence, and the bytes are already in hand.

## `--show-output` is spelled `task-log`

What a caller reaching for `--show-output` wants is upstream's `taskLog`: rows dimmed with
`styleText('dim', line)`, a window of the last `limit` of them, `stripDestructiveANSI` over each —
a function that exists only because the rows come from somebody else's program — and the whole log
erased when the task succeeds. Upstream's own example is a command's output fed in line by line.

It is already ported. All the CLI adds is the reading:

```sh
clark task-log "Ripping the CD" --limit 5 -- ffmpeg -i track.wav track.flac
```

One reader thread per stream feeds one `Mutex<TaskLog>`. Rows within a stream keep their order and
the two streams interleave as they arrive; ordering them exactly means handing both file descriptors
one pipe, which cannot be done before the spawn without `os_pipe`. If the interleaving turns out to
matter, that is the upgrade.

`progress` is still not here. `spinner` and `task-log` both end when the command does, which clark
can see for itself. A bar has to be told how far along it is, and a wrapped command has no way back
to say so — that needs a channel, and a channel is a different decision.

## `output` comes back

`clark-core` writes nothing; only `clark` does, and it chose its stream at compile time —
`crates/clark/src/driver.rs` on stderr for Prompts, `message.rs` on stdout for the static renderers,
`ticker.rs` on stdout for all three of these.

Stdout is wrong for a wrapped command. `clark spinner "…" -- make > build.log` would put frames in
the log, and `$(…)` would capture the animation.

Upstream has the knob: `output?: Writable` in `CommonOptions`, defaulting to `process.stdout`. The
port had dropped it. It is back on the three builders as
[`Output`](../../crates/clark/src/ticker.rs), which is an enum and not a writer — a terminal program
has two streams and no caller has ever passed a third, whereas a generic would reach through
`Ticker`'s `Arc<Mutex<T>>` and every builder holding one. The default is upstream's, so nothing
already written changes; `clark-cli` asks for stderr.

One thing moved with it. `task_log()` read `isTTY` from stdout in the constructor, which is the
wrong stream as soon as the caller can pick one, so the probe now runs in `start()` against whatever
`output` ended up being. `quiet()` still overrides it.

## Notes

- **The exit code is the command's.** `clark spinner "…" -- make && …` reads the way it looks. A
  child killed by a signal reports no code; `ctrl+c` reaches the whole process group, so that is the
  cancel — drawn before the exit rather than through `Failure::Cancelled`, whose arm leaves no row.
- **Both spawn before they draw.** A command that cannot run at all is a bad argument, and reads
  like every other one: `clark: …` on stderr, exit `2`, and no row left behind.
- **`block()` is still not ported**, so keys typed while a wrapped command runs echo into the live
  region. The reasoning in `crates/clark/src/spinner.rs` is unchanged by the CLI having arrived; a
  reader thread competing with the caller's stdin is still the larger promise.
- **No new dependencies.** `std::process` covers the whole of it.
