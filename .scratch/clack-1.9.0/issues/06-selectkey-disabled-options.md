# 06: selectKey disabled options (clack #622)

**What to build:** selectKey accepts disabled options. A disabled option is drawn as a dim ` key ` badge, a gray label and a dim `(hint)` if it has one. Pressing its key does nothing, and a disabled initial value is ignored, leaving the cursor on the first option. clark-cli can mark a selectKey option disabled, the same way it already can for select.

**Blocked by:** 02, 03 (same widget code; the select-key Fixtures need both changes)

**Status:** done

- [x] A disabled option renders with the styling above, the hint included when present
- [x] Its key press neither resolves nor redraws the Prompt as if it had been pressed
- [x] A disabled initial value puts the cursor on the first option
- [x] Unit tests cover the key press and the initial value
- [x] clark-cli exposes disabled selectKey options and its README documents them
- [x] The select-key suite passes
