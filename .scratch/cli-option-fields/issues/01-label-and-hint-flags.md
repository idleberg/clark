# 01: Labels and hints for clark-cli options

**What to build:** clark-cli can give a list option a label and a hint. In clack, `Option<Value>` has four fields: `value`, `label`, `hint`, `disabled`. Since clack 1.9.0 issue 06, the CLI can set `value` (`--option`, or one per line on stdin) and `disabled` (`--disabled <value>`). It cannot set the other two, so a label is always the bare value and no hint is ever drawn. In `select-key` the value is also the key badge, so `--option deploy` draws ` deploy  deploy`.

The two fields get different mechanisms, because they are used differently.

**Labels: `--label-delimiter <sep>`, as in gum.** gum's `choose` has the same flag ("options can be set as label:value"). Off by default. When it is set, every option, from `--option` or from stdin, is split on the first `<sep>`: the left side is the label and the rest is the value. A label can't contain `<sep>`, but a value can. The caller picks a delimiter that their data doesn't contain, so values with colons in them, such as URLs and Windows paths, still work. Labels usually come in with a whole piped list, and this is the only shape that works on stdin:

```sh
printf 'Deploy:d\nDelete:x\n' | clark select-key "Action?" --label-delimiter :
```

Label first rather than value first, so a gum user's pipeline carries over unchanged. An option that doesn't contain the delimiter is an error. It is probably a missed separator, not an option meant to have no label.

**Hints: repeatable `--hint <value>=<text>`, keyed like `--disabled`.** Hints go on one or two options, not on a whole list, and gum has no equivalent to copy. The split is on the first `=`. A value that names no option is an error.

Every key goes by value, never by label: `--disabled`, `--hint`, `--initial-value` and the printed answer. Both flags go on the shared `Options` struct, so they reach `select`, `multiselect`, `select-key`, `autocomplete` and `autocomplete-multiselect`, the same prompts whose clack `Option` has these fields.

**Open question:** `group-multiselect`'s options come from `--group 'Heading:a,b'` strings and bypass `Options`. Does `--label-delimiter` apply inside a group? A `:` delimiter would collide with the group's own `:`.

**Blocked by:** none

**Status:** ready-for-agent

- [ ] `--label-delimiter` splits every option, from flags or stdin, into label and value on the first delimiter
- [ ] An option without the delimiter is an error while the flag is set
- [ ] `--hint value=text` sets that option's hint; a value that names no option is an error
- [ ] `--disabled`, `--hint` and `--initial-value` match values, not labels
- [ ] Unit tests cover the split, both errors, and keys matching values
- [ ] The clark-cli README documents both flags, with a piped `select-key` example
- [ ] `group-multiselect` decided: in scope or explicitly out
