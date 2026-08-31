// The hand-authored Scenarios: what clack does at widths its own test suite never uses.
//
// Every harvested Scenario runs at one width and none of them resizes, because upstream's tests
// have no reason to vary the terminal. Wrapping and re-layout are therefore the least-tested paths
// in the port and the two places `session.rs` records a known divergence. These cases exist to put
// a recording under them.
//
// A case is upstream's input and nothing else — options, a terminal width, a sequence of
// keypresses, and what `text()` should return. No expected output appears here: that is the whole
// point of a Scenario, and the bytes are whatever clack writes when the case is run.
//
// `value` is not decoration. A hand-authored Scenario has no upstream snapshot behind it, so the
// only evidence that the keys drove clack anywhere is what clack handed back; ./record.test.mjs
// asserts it and refuses to write a recording that fails.

/** A run of ordinary characters, as readline reports them: the character, named after itself. */
const typing = (text) => [...text].map((s) => ({ s, key: { name: s } }));

const enter = { s: '', key: { name: 'return' } };
const escape = { s: '', key: { name: 'escape' } };
const backspace = (n) => Array.from({ length: n }, () => ({ s: '', key: { name: 'backspace' } }));

/** Wide enough that the prompt's own `◇  ` prefix matters, narrow enough to wrap a short sentence. */
const NARROW = 40;

export const cases = [
	{
		// The message is upstream's own longest realistic question, at a width it does not fit.
		name: 'narrow › the message wraps',
		columns: NARROW,
		opts: { message: 'What is the name of the package you would like to publish?' },
		keys: [...typing('acme'), enter],
		value: 'acme',
	},
	{
		// The value crosses the width while it is being typed, so the Frame gains a row between two
		// keypresses. Upstream re-wraps the previous Frame to count the rows it walks back over;
		// the port keeps the rows it laid out. Growing is the direction the two agree on.
		name: 'narrow › the value wraps as it is typed',
		columns: NARROW,
		opts: { message: 'Path' },
		keys: [...typing('packages/prompts/src/prompts/text-input.ts'), enter],
		value: 'packages/prompts/src/prompts/text-input.ts',
	},
	{
		// And shrinking is the direction they might not. Deleting back across the wrap takes a row
		// away, which is the erase path rather than the write path.
		name: 'narrow › deleting back across the wrap',
		columns: NARROW,
		opts: { message: 'Path' },
		keys: [...typing('packages/prompts/src/prompts/text-input.ts'), ...backspace(22), enter],
		value: 'packages/prompts/src',
	},
	{
		// Cancelled, wrapped, and struck through: the one Theme attribute `vt100` cannot see, on a
		// value long enough to need two rows. See ADR-0015 for why the emulator is `avt`.
		name: 'narrow › a cancelled value wraps',
		columns: NARROW,
		opts: { message: 'Path' },
		keys: [...typing('packages/prompts/src/prompts/text-input.ts'), escape],
		cancelled: true,
	},
	{
		// Wide characters at a width they fit: nothing wraps, and the only question is whether the
		// port agrees with `string-width` about how much of the row they occupy.
		name: 'cjk › a wide message and a wide value',
		columns: 80,
		opts: { message: '新しいパッケージの名前は何ですか' },
		keys: [...typing('日本語'), enter],
		value: '日本語',
	},
	{
		// A wide character cannot straddle the wrap. With the `◇  ` prefix the usable width is odd,
		// so the boundary falls in the middle of a two-column character and something has to give.
		name: 'cjk › a wide message wraps where a character cannot split',
		columns: NARROW,
		opts: { message: '新しいパッケージの名前を入力してください、よろしくお願いします' },
		keys: [...typing('acme'), enter],
		value: 'acme',
	},
	{
		// The same question about the value rather than the message, and the cursor sits after a
		// wide character rather than a narrow one.
		name: 'cjk › a wide value wraps',
		columns: 20,
		opts: { message: 'Name' },
		keys: [...typing('日本語テキストの入力'), enter],
		value: '日本語テキストの入力',
	},
];
