# A path is an autocomplete whose filter is a filesystem

M5's last unit: `path`. It adds no widget, no state machine and no keypress. `path.ts` is forty
lines, thirty of which are one callback, and it is the only module in clack that reads something
outside the terminal — which is why it was the last thing left in v2 and why the README has owed an
`Fs` since M0.

[`clackatui_core::path`](../../clackatui-core/src/path.rs) is the port;
[`clackatui::path`](../../clackatui/src/path.rs) is the builder and the one thing in the project
that opens a directory.

## The list is not filtered, it is re-read

`AutocompletePrompt`'s `options` may be an array *or a function*, and the two are not the same
Prompt. The constructor writes

```ts
this.#filterFn = typeof opts.options === 'function' ? opts.filter : (opts.filter ?? defaultFilter);
```

— the fallback filter is the array form's alone. `path` passes a function and no filter, so it runs
with **no filter at all**: `#onUserInputChanged` takes its `else` branch and keeps everything the
function returned. Every keystroke calls the function again, and the narrowing is done by
`readdirSync` and a `startsWith`.

So the seam is not "a callback in front of the filter". It is
[`AutocompleteState::with_options_fn`](../../clackatui-core/src/autocomplete.rs), and it carries
`filter: Option<Box<Filter<T>>>` with it — a genuinely absent filter, because upstream's is
genuinely absent. The provider is called wherever upstream reads its `options` getter: once on the
way in, against an empty field, because `initialUserInput` is applied after the constructor; and
once for every change to the text. There is a third read, in the `tab` branch, whose answer is used
for one question and thrown away — so the port asks there too, and only when there is a placeholder
to ask about, because asking is a filesystem call.

`filteredOptions` upstream holds the options themselves; here it holds positions into the list, so a
list that is replaced has to replace the positions with it. That is the one place this seam is more
than a callback.

## `Fs` is three questions

`existsSync`, `lstatSync().isDirectory()` and `readdirSync`. They are separate because upstream asks
them separately and branches between them: a path that does not exist is listed by its parent, a
path that does is listed by itself or by its parent depending on what it is and on whether the text
ends in a slash. `read_dir` answers `None` where `readdirSync` throws, which is upstream's `catch`
and is reachable by typing.

The trailing slash is the whole of `directory` mode. A directory named without one lists its
*siblings*, so enter answers with the directory itself; type the slash and the children appear. Two
of upstream's cases are exactly that pair.

`clackatui::StdFs` answers the three with `std::fs`, `symlink_metadata` for both of the first two —
`lstat`, not `stat`, so a symlink to a directory is offered as a leaf. It does not sort, because
`readdirSync` does not sort.

## `node:path`, not `std::path`

`dirname` decides which directory is listed after every keystroke, so it is a specification rather
than an implementation detail. It is not `Path::parent`: node answers `"."` where Rust answers
`None`, and node's trailing-slash scan drops a component Rust keeps. Both `dirname` and `join` are
ported from node's posix versions as they are written, `"//"` case and all.

Posix on every platform. `path.ts` gets whichever `node:path` the platform hands it, and the corpus
behind this module is posix; guessing at the other one is how a port grows a behaviour nobody
recorded.

## The recording carries the filesystem

Upstream mocks `node:fs` with `memfs` and builds a volume per case, so a recording of a `path` run
that does not carry the volume is a recording of a list with nothing behind it — the Recorder would
write the options down as empty and the port would agree, having read the same nothing.

So the Recorder reads `vol.toJSON()` as each prompt opens and writes it into the run. That is an
observation like the keypresses, not an arrangement: it happens after the test has built its volume
and changes nothing. It has to be *the same* `memfs` the suite imports, which is a module-identity
problem under pnpm and is solved the way the entry point already was — an alias, `clack:memfs`,
resolved from clack's own package. The Scenarios rebuild the volume as
[`MemFs`](../../clackatui-core/src/path.rs), which sorts its listings because `memfs` sorts and a
real filesystem does not. That difference is upstream's and it decides which suggestion is first,
and so what a bare enter answers with.

## Consequences

- **Thirteen Scenarios, thirty-six keypresses**, from a suite whose `describe` is named `text` —
  the same copy-paste `selectKey`'s carries, and the Fixture keeps it, because renaming a case is
  editing the evidence.
- **Two of the three things `path()` writes for itself** are supplied by the loader rather than
  carried by a Scenario: `maxItems: 5` and the validator that refuses an empty answer. The
  `multiselect` bargain again. The third is `initialUserInput`, which the recording does carry.
- **`Run` gained no variant.** A `path` is a `Session<AutocompleteState<String>>`, which is what
  `Run::Autocomplete` already holds.
- **The `Fs` the README owed is paid.** What is left of the deferred set is `task` and `stream`,
  neither of which is a Prompt.
- **Thirty-seven mutants, thirty-three caught.** Six of the first run's ten survivors were gaps and
  are now unit tests, all of them about text that no recording types: a slash after a *file*, which
  is the case upstream's own comment on the prefix is written for; node's `normalize` above a root
  and its trailing slash, neither of which a `readdirSync` entry can produce; a `MemFs` asked to list
  something that is not a directory; and the constructor's call to the options function, which is
  made against an empty field and then thrown away by `initialUserInput`.
- **The last four are equivalent**, and each says something about the seam. `existsSync` is redundant
  here — it guards a `lstatSync` that throws upstream, and `Fs::is_dir` answers `false` instead, so
  both branches lead to the parent. The `length > 1` on the prefix is unreachable with effect: the
  only one-character path that ends in a slash is `/`, and everything under `/` starts with the empty
  prefix too. A directory that cannot be read and one that is empty suggest the same nothing. And the
  provider cannot be asked against the old text, because the new text has been written into
  `#lastUserInput` one line above.
