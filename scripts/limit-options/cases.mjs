// The corpus `limitOptions` is harvested against. Read by ./record.test.mjs, which is run by
// scripts/harvest-limit-options.mjs.
//
// `limitOptions` is a pure function of eight numbers and a list, which makes it the first thing
// since the wrap port that can be given a corpus rather than a recording (ADR-0008). It is also the
// most arithmetic in clack: a sliding window, two ellipsis rows that are decided before the trim and
// again after it, and a trim that walks outwards from the cursor's group in an order that depends on
// which ellipsis is already there. Fourteen upstream tests reach some of that. The corpus below
// exists to reach the rest.
//
// # Shape of a case
//
//   name           unique; the Rust side reports failures by it
//   options        one entry per option. A `\n` in one is a multi-line label, which upstream
//                  supports and which is the only way a group can be more than one line before the
//                  wrap gets to it.
//   cursor         index of the active option. Not clamped upstream, and not clamped here.
//   columns, rows  the terminal, as `getColumns`/`getRows` read it off the output stream.
//   maxItems       null for upstream's default of Infinity.
//   columnPadding  null for upstream's default of 0.
//   rowPadding     null for upstream's default of 4.
//   style          one of the named styles below, because a fixture cannot carry a closure.
//
// # The styles
//
//   plain    the option itself. What eleven of upstream's fourteen tests use.
//   marker   `> ` before the active option and two spaces before the others, so a case can tell
//            whether the port passed the right index to the callback and not merely the right
//            number of them.
//   wide     the option wrapped in `-- … --`, which pushes it over a narrow margin and makes the
//            case about the wrap rather than about the window.
//
// Every string is written as an array of code points, for the reason harvest-width.mjs gives: a
// decomposed sequence written literally gets silently precomposed somewhere between the editor and
// the filesystem, and then the fixture is not what it claims.

/** ASCII and the shapes clack draws are safe to write literally: nothing normalises them. */
const a = (s) => [...s].map((c) => c.codePointAt(0));

/** `n` options named `Item 1` … `Item n`, which is how upstream's own tests spell a list. */
const items = (n) => Array.from({ length: n }, (_, i) => a(`Item ${i + 1}`));

/** The four-line option upstream uses to test multi-line clamping. */
const TALL = a(
	Array.from({ length: 4 })
		.map((_v, i) => `A long item that will take up a lot of space (line ${i})`)
		.join('\n')
);

/**
 * @param {string} name
 * @param {object} c
 */
const c = (name, c) => ({
	name,
	options: c.options,
	cursor: c.cursor ?? 0,
	columns: c.columns ?? 80,
	rows: c.rows ?? 20,
	maxItems: c.maxItems ?? null,
	columnPadding: c.columnPadding ?? null,
	rowPadding: c.rowPadding ?? null,
	style: c.style ?? 'plain',
});

export const cases = [
	// --- nothing to do ---------------------------------------------------------------------------
	c('empty list', { options: [] }),
	c('empty list with a cursor on nothing', { options: [], cursor: 3 }),
	c('one option', { options: items(1) }),
	c('everything fits', { options: items(3), maxItems: 5 }),
	c('everything fits with no maxItems at all', { options: items(3) }),
	c('exactly as many options as rows allow', { options: items(16) }),
	c('one option more than rows allow', { options: items(17) }),

	// --- the floor of five -----------------------------------------------------------------------
	// `Math.max(Math.min(maxItems, outputMaxItems), 5)` — a maxItems below five is ignored, and so
	// is a terminal too short for five. The second is the one no upstream test reaches.
	c('maxItems below the floor', { options: items(7), maxItems: 3 }),
	c('maxItems of one', { options: items(7), maxItems: 1 }),
	c('maxItems of zero', { options: items(7), maxItems: 0 }),
	c('a terminal shorter than the padding', { options: items(7), rows: 3 }),
	c('a terminal exactly as tall as the padding', { options: items(7), rows: 4 }),
	c('a terminal one row taller than the padding', { options: items(7), rows: 5 }),
	c('the floor beats the terminal', { options: items(9), rows: 6, maxItems: 8 }),

	// --- the sliding window ------------------------------------------------------------------------
	// The window starts moving at `cursor >= computedMaxItems - 3`, which is two rows before the
	// bottom of it rather than at the bottom. Walking a cursor down a list is how that shows.
	...Array.from({ length: 10 }, (_v, i) =>
		c(`cursor at ${i} of ten with a window of five`, {
			options: items(10),
			rows: 20,
			maxItems: 5,
			cursor: i,
		})
	),
	c('cursor past the end of the list', { options: items(10), maxItems: 5, cursor: 12 }),
	c('cursor on the last option of a long list', { options: items(30), maxItems: 7, cursor: 29 }),
	c('a window as large as the list', { options: items(6), maxItems: 6, cursor: 5 }),
	c('a window one smaller than the list', { options: items(6), maxItems: 5, cursor: 5 }),

	// --- rowPadding --------------------------------------------------------------------------------
	// `select` passes the height of its own title and footer here, so a wrapped message shortens the
	// list. Nothing upstream varies it except the two tests at the bottom of its suite.
	c('rowPadding of zero', { options: items(10), rows: 12, rowPadding: 0 }),
	c('rowPadding of six', { options: items(10), rows: 12, rowPadding: 6 }),
	c('rowPadding of six, scrolled', { options: items(10), rows: 12, rowPadding: 6, cursor: 5 }),
	c('rowPadding larger than the terminal', { options: items(10), rows: 8, rowPadding: 12 }),

	// --- columnPadding and the wrap ------------------------------------------------------------------
	// Every option is wrapped to `columns - columnPadding` before it is counted, so a padding wide
	// enough turns one option into several rows and the trim has to deal with groups of uneven
	// height. `select` passes 13 here — the length of a styled bar and two spaces (ADR-0019).
	c('one option too wide for the terminal', {
		options: [a('a rather long option that does not fit')],
		columns: 20,
	}),
	c('every option too wide', { options: items(6), columns: 8, style: 'wide' }),
	c('the guide prefix taken off the width', {
		options: [a('a rather long option that does not fit')],
		columns: 30,
		columnPadding: 13,
	}),
	c('a padding as wide as the terminal', { options: items(3), columns: 10, columnPadding: 10 }),
	c('a padding wider than the terminal', { options: items(3), columns: 10, columnPadding: 12 }),
	c('wrapped options overflow a short terminal', {
		options: items(6),
		columns: 12,
		rows: 10,
		style: 'wide',
		cursor: 4,
	}),

	// --- multi-line options ---------------------------------------------------------------------------
	// A group is one option and may be several rows, so `lineCount` and the number of options part
	// company. This is where the second pass of the trim lives, and it is the only part of the
	// function that can remove an option the window had already decided to keep.
	c('a tall option at the top', { options: [TALL, ...items(9)], rows: 14, maxItems: 10 }),
	c('a tall option in the middle', {
		options: [...items(4), TALL, ...items(5)],
		rows: 14,
		maxItems: 10,
		cursor: 7,
	}),
	c('a tall option at the end', {
		options: [...items(7), TALL, ...items(2)],
		rows: 14,
		maxItems: 10,
		cursor: 9,
	}),
	c('the cursor is on the tall option', {
		options: [...items(4), TALL, ...items(5)],
		rows: 14,
		maxItems: 10,
		cursor: 4,
	}),
	c('the cursor is on a tall option taller than the terminal', {
		options: [...items(4), TALL, ...items(5)],
		rows: 8,
		maxItems: 10,
		cursor: 4,
	}),
	c('every option is two lines', {
		options: Array.from({ length: 8 }, (_v, i) => a(`Item ${i + 1}\ncontinued`)),
		rows: 14,
		maxItems: 8,
		cursor: 6,
	}),
	c('a blank line inside an option', { options: [a('Item 1\n\nItem 1 again'), ...items(4)] }),
	c('an option that is only a newline', { options: [a('\n'), ...items(4)] }),
	c('an empty option', { options: [[], ...items(4)] }),

	// --- the style callback ---------------------------------------------------------------------------
	c('the marker follows the cursor', { options: items(3), cursor: 1, style: 'marker' }),
	c('the marker on a scrolled window', {
		options: items(10),
		maxItems: 5,
		cursor: 7,
		style: 'marker',
	}),
	c('the marker on a multi-line option', {
		options: [a('Item 1\nContinued'), ...items(2)],
		cursor: 0,
		style: 'marker',
	}),

	// --- widths that are not one -------------------------------------------------------------------
	c('cjk options', { options: [[0x4f60, 0x597d], [0x4e16, 0x754c], ...items(2)], columns: 6 }),
	c('cjk against an odd margin', { options: [[0x4f60, 0x597d, 0x4f60]], columns: 5 }),
	c('an emoji option at a narrow margin', {
		options: [[0x1f600, ...a(' ok')], ...items(2)],
		columns: 4,
	}),
	c('a combining mark at the margin', { options: [[0x0061, 0x0062, 0x0301, 0x0063]], columns: 2 }),
];
