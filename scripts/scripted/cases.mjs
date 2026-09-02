// The corpus for the renderers that are driven by calls rather than by keys.
//
// A Prompt is a state machine fed keypresses; a `log` is one call and one string. A spinner is
// neither: it is an object a program calls `start`, `message` and `stop` on, and a clock that draws
// between those calls. So a case here is a *script* — a list of steps — and what is recorded is the
// bytes each step wrote.
//
// A `tick` is one turn of the interval, which the Recorder produces by advancing a fake clock by the
// spinner's own `delay`. That makes the recording deterministic in the one place it could not
// otherwise be: `indicator: 'timer'` prints the time since `start`, and every case that asks for one
// sets a `delay` that puts the seconds where they can be read.
//
// Every case ends its spinner. A spinner that is never stopped leaves Node's exit handlers
// registered, and the Recorder would be recording the listener leak as well as the bytes.

/** A script, with the defaults every case shares. */
const spin = (name, steps, options = {}, columns = 80) => ({
	name,
	kind: 'spinner',
	options,
	columns,
	rows: 20,
	steps,
});

/** The same, for `progress()` — a spinner whose message is a bar. */
const bar = (name, steps, options = {}, columns = 80) => ({
	...spin(name, steps, options, columns),
	kind: 'progress',
});

/**
 * A `taskLog` script. Every one of them opens, because `taskLog()` writes its title from the
 * constructor and those bytes belong to a step like any other.
 *
 * `tty` follows `ci` unless a case says otherwise: what a task log calls `isTTY` is
 * `!isCI() && output.isTTY`, and a real CI runner has neither.
 */
const task = (name, steps, options = {}, columns = 80) => ({
	name,
	kind: 'task-log',
	options: { title: 'build', ...options },
	columns,
	rows: 20,
	steps: [{ op: 'open' }, ...steps],
});

const say = (message, raw) => ({ op: 'message', message, ...(raw === undefined ? {} : { raw }) });
const group = (name) => ({ op: 'group', name });
/** A message inside the `n`th group the script made. */
const groupSay = (group, message, raw) => ({ ...say(message, raw), op: 'group-message', group });
const done = (message = 'finished', showLog) => ({
	op: 'success',
	message,
	...(showLog === undefined ? {} : { showLog }),
});
const failed = (message = 'broken', showLog) => ({
	op: 'error',
	message,
	...(showLog === undefined ? {} : { showLog }),
});

/** An escape, for the messages that carry one. */
const ESC = '\u001b';

const start = (message = '') => ({ op: 'start', message });
const tick = (count = 1) => Array.from({ length: count }, () => ({ op: 'tick' }));
const message = (message) => ({ op: 'message', message });
const stop = (message = '') => ({ op: 'stop', message });
/** `advance(step, msg?)`. A `message` of `undefined` is the one live fallback in either module. */
const advance = (step = 1, message) => ({ op: 'advance', step, message });

export const cases = [
	// --- the shape of one run -----------------------------------------------------------------

	spin('a spinner draws a bar, four frames and a submitted step', [
		start('Loading'),
		...tick(4),
		stop('Done'),
	]),
	spin('the frames come back round after the fourth', [start('Loading'), ...tick(9), stop('Done')]),
	spin('a spinner stopped with no message keeps only the symbol', [
		start('Loading'),
		...tick(1),
		stop(),
	]),
	spin('a spinner that is stopped before it ticks writes no row to erase', [start('Loading'), stop('Done')]),
	{
		// The one case that records a renderer writing nothing. `_stop` returns on `!isSpinnerActive`
		// before it touches the output, so there is nothing on the wire and nothing on the terminal —
		// which is exactly the shape a case that silently failed to run would have, hence the flag.
		...spin('stopping a spinner that was never started writes nothing at all', [stop('Done')]),
		expectsNothing: true,
	},
	spin('a cancelled spinner ends in red', [
		start('Loading'),
		...tick(1),
		{ op: 'cancel', message: 'too dizzy — spinning cancelled' },
	]),
	spin('an errored spinner ends in the other red', [
		start('Loading'),
		...tick(1),
		{ op: 'error', message: 'error: spun too fast!' },
	]),
	spin('a cleared spinner erases its row and leaves the bar', [
		start('Loading'),
		...tick(1),
		{ op: 'clear' },
	]),
	spin('a message set mid-flight is drawn by the next tick and not before', [
		start('Loading'),
		...tick(1),
		message('Still loading'),
		...tick(2),
		stop('Done'),
	]),
	spin('a message set after the last tick is never drawn', [
		start('Loading'),
		...tick(1),
		message('never seen'),
		stop('Done'),
	]),
	// Ten ticks is far enough into the dots that a second `start` which did not reset the counter
	// would draw one on its very first frame.
	spin('a spinner started again keeps the message the first one left behind', [
		start('first'),
		...tick(10),
		stop('done'),
		start('second'),
		...tick(2),
		stop('done again'),
	]),

	// --- the dots -----------------------------------------------------------------------------

	spin('the dots arrive one every eight ticks and are capped at three', [
		start('Loading'),
		...tick(34),
		stop('Done'),
	]),
	spin('the dots go round again after the cap', [start('Loading'), ...tick(70), stop('Done')]),
	// Only the trailing ones: the regular expression is anchored at the end.
	spin('leading dots are left where they are', [
		start('...Loading...'),
		...tick(2),
		stop('...Done...'),
	]),
	spin('trailing dots are taken off the message they were asked for', [
		start('Loading...'),
		...tick(2),
		stop('Done.'),
	]),
	spin('a message set mid-flight loses its dots too', [
		start('Loading'),
		...tick(1),
		message('Almost there...'),
		...tick(1),
		stop('Done'),
	]),
	spin('a spinner with no message at all is a symbol and two spaces', [
		start(),
		...tick(3),
		stop(),
	]),

	// --- the timer ----------------------------------------------------------------------------

	spin(
		'a timer counts the seconds since start',
		[start('Working'), ...tick(4), stop('Done')],
		{ indicator: 'timer', delay: 1000 }
	),
	spin(
		'a timer counts minutes once there are any',
		[start('Working'), ...tick(4), stop('Done')],
		{ indicator: 'timer', delay: 20000 }
	),
	spin(
		'a timer with a delay that is not a whole second floors it',
		[start('Working'), ...tick(6), stop('Done')],
		{ indicator: 'timer', delay: 400 }
	),
	spin(
		'a timer is drawn on the closing row as well',
		[start('Working'), ...tick(2), { op: 'cancel', message: 'gave up' }],
		{ indicator: 'timer', delay: 1000 }
	),

	// --- the Guide, and the frames ------------------------------------------------------------

	spin('without the Guide there is no bar above the spinner', [start('foo'), ...tick(1), stop()], {
		withGuide: false,
	}),
	spin(
		'custom frames are cycled in the order they were given',
		[start('Loading'), ...tick(5), stop('Done')],
		{ frames: ['-', '\\', '|', '/'] }
	),
	spin(
		'frames two columns wide move the message and the wrap with them',
		[start('Loading'), ...tick(5), stop('Done')],
		{ frames: ['🐴', '🦋', '🐙', '🐶'] }
	),
	spin(
		'ten frames still show the same three dots',
		[start('Loading'), ...tick(64), stop('Done')],
		{ frames: ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'] }
	),
	spin('a frame can be styled by the caller', [start('Loading'), ...tick(4), stop('Done')], {
		styleFrame: 'red',
	}),
	spin(
		'a frame formatter that adds characters is on the wire before the wrap',
		[start('Loading'), ...tick(2), stop('Done')],
		{ styleFrame: 'stars' }
	),

	// --- what the wrap does to the walk back --------------------------------------------------

	spin('a multi-line message is drawn and walked back over row by row', [
		start('foo\nbar\nbaz'),
		...tick(2),
		stop('Done'),
	]),
	spin('a message longer than the terminal wraps', [
		start('x'.repeat(90)),
		...tick(2),
		stop('stopped'),
	]),
	// The three columns the prefix costs are not counted when the cursor walks back. A message that
	// fits and a drawn row that does not is the whole of the defect, so both sides of the boundary
	// are here.
	spin('a message three columns short of the terminal', [
		start('x'.repeat(17)),
		...tick(3),
		stop('Done'),
	], {}, 20),
	spin('a message one column short of the terminal', [
		start('x'.repeat(19)),
		...tick(3),
		stop('Done'),
	], {}, 20),
	spin('a message exactly as wide as the terminal', [
		start('x'.repeat(20)),
		...tick(3),
		stop('Done'),
	], {}, 20),
	spin('a message that wraps to three rows in a narrow terminal', [
		start('aaaa bbbb cccc dddd eeee ffff'),
		...tick(3),
		stop('Done'),
	], {}, 20),
	spin('a message of wide characters in a narrow terminal', [
		start('メッセージがとても長いです'),
		...tick(3),
		stop('Done'),
	], {}, 20),
	spin('a message that shrinks between ticks', [
		start('a much longer message than the one that follows it'),
		...tick(1),
		message('short'),
		...tick(1),
		stop('Done'),
	], {}, 20),
	spin('a message that grows between ticks', [
		start('short'),
		...tick(1),
		message('a much longer message than the one that came before it'),
		...tick(1),
		stop('Done'),
	], {}, 20),
	spin(
		'a stop message longer than the terminal is not wrapped',
		[start('Loading'), ...tick(1), stop('y'.repeat(90))],
		{},
		20
	),

	// --- CI ------------------------------------------------------------------------------------

	spin('in CI a tick writes a newline, the message and three dots', [
		start('Loading'),
		...tick(4),
		stop('Done'),
	], { ci: true }),
	spin('in CI only a changed message writes anything', [
		start('Loading'),
		...tick(2),
		message('Still loading'),
		...tick(2),
		stop('Done'),
	], { ci: true }),
	spin('in CI a multi-line message is still walked back over', [
		start('foo\nbar'),
		...tick(2),
		message('baz'),
		...tick(1),
		stop('Done'),
	], { ci: true }),
	// In CI the tick's own branch wins and the row is `${frame}  ${msg}...` whatever the indicator
	// says; only the closing row reads it.
	spin('in CI the ticks show no timer and the closing row does', [start('Working'), ...tick(3), stop('Done')], {
		ci: true,
		indicator: 'timer',
		delay: 1000,
	}),
	spin('in CI a cleared spinner still writes its newline', [
		start('Loading'),
		...tick(1),
		{ op: 'clear' },
	], { ci: true }),

	// --- a progress bar is a spinner whose message is a bar --------------------------------------

	bar('a bar starts empty and fills as it is advanced', [
		start('Downloading'),
		...tick(1),
		advance(25),
		...tick(1),
		advance(25),
		...tick(1),
		advance(50),
		...tick(1),
		stop('Downloaded'),
	]),
	// A max of four so that a step of one is a column: at the default max of a hundred and a size of
	// forty, `advance()` moves the bar by four tenths of a column and draws the same row twice.
	bar('a bar advances by one when it is not told how far', [
		start('Working'),
		...tick(1),
		advance(),
		...tick(1),
		advance(),
		...tick(1),
		stop('Done'),
	], { max: 4, size: 4 }),
	bar('a bar advanced past its max stops at full', [
		start('Working'),
		...tick(1),
		advance(500),
		...tick(1),
		advance(500),
		...tick(1),
		stop('Done'),
	]),
	bar('a bar advanced with a message of its own draws that one', [
		start('first'),
		...tick(1),
		advance(50, 'second'),
		...tick(1),
		advance(25),
		...tick(1),
		stop('Done'),
	]),
	// `message` is `advance(0, msg)`, so the bar is redrawn exactly where it was.
	bar('a message on a bar advances nothing', [
		start('first'),
		...tick(1),
		advance(30),
		...tick(1),
		message('second'),
		...tick(1),
		stop('Done'),
	]),
	bar('a bar with trailing dots on its message loses them and keeps its bar', [
		start('Loading...'),
		...tick(2),
		advance(40, 'Almost there...'),
		...tick(1),
		stop('Done.'),
	]),

	// --- the three styles, the two numbers -------------------------------------------------------

	bar('a light bar', [start('go'), ...tick(1), advance(50), ...tick(1), stop('done')], {
		style: 'light',
		size: 10,
	}),
	bar('a heavy bar is the default', [start('go'), ...tick(1), advance(50), ...tick(1), stop('done')], {
		size: 10,
	}),
	bar('a block bar', [start('go'), ...tick(1), advance(50), ...tick(1), stop('done')], {
		style: 'block',
		size: 10,
	}),
	// Seven tenths of ten columns is the arithmetic that has to floor the way upstream's floors.
	bar(
		'a max and a size that do not divide evenly',
		[start('go'), ...tick(1), advance(7), ...tick(1), advance(1), ...tick(1), stop('done')],
		{ max: 10, size: 3 }
	),
	bar('a bar of one column and a max of one', [start('go'), ...tick(1), advance(1), ...tick(1), stop('done')], {
		max: 1,
		size: 1,
	}),
	// `Math.max(1, …)` on both, so neither can be nothing. A max of nothing is the interesting half:
	// one step fills the whole bar, and a port that left the max at zero would draw the same
	// characters in the other colour — which is why this one is wide enough to wrap. The row the
	// walk back fails to erase is the only place a colour survives to the end of a run.
	bar('a bar asked for no columns and no max gets one of each', [
		start('go'),
		...tick(2),
		advance(1),
		...tick(1),
		stop('done'),
	], { max: 0, size: 0 }),
	bar('a bar with a max of nothing is filled by one step', [
		start(''),
		...tick(1),
		advance(1),
		...tick(2),
		stop('done'),
	], { max: 0, size: 17 }, 20),

	// --- the bar is in the message, so the wrap and the walk back see it -------------------------

	bar(
		'a bar wider than the terminal wraps and is walked back over',
		[start('Downloading'), ...tick(2), advance(50), ...tick(2), stop('Done')],
		{ size: 30 },
		20
	),
	// The bar and its space are eighteen columns of a twenty-column terminal, so the message is one
	// row and the row that was drawn is two. The three columns the frame costs are the ones
	// `clearPrevMessage` does not count.
	bar(
		'a bar two columns short of the terminal',
		[start(''), ...tick(3), advance(50), ...tick(2), stop('Done')],
		{ size: 17 },
		20
	),
	bar(
		'a bar of wide characters',
		[start('進捗'), ...tick(2), advance(50), ...tick(2), stop('完了')],
		{ size: 8 },
		20
	),

	// --- everything the spinner underneath still does --------------------------------------------

	bar('the dots still arrive under a bar', [
		start('Working'),
		...tick(20),
		advance(50),
		...tick(20),
		stop('Done'),
	], { size: 10 }),
	bar(
		'a bar with a timer instead of dots',
		[start('Working'), ...tick(2), advance(50), ...tick(2), stop('Done')],
		{ size: 10, indicator: 'timer', delay: 1000 }
	),
	bar('a bar without the Guide', [start('go'), ...tick(1), advance(50), ...tick(1), stop('done')], {
		size: 10,
		withGuide: false,
	}),
	bar('a cancelled bar', [
		start('Working'),
		...tick(1),
		advance(30),
		...tick(1),
		{ op: 'cancel', message: 'gave up' },
	], { size: 10 }),
	bar('an errored bar', [
		start('Working'),
		...tick(1),
		advance(30),
		...tick(1),
		{ op: 'error', message: 'failed' },
	], { size: 10 }),
	bar('a cleared bar leaves only the Guide', [
		start('Working'),
		...tick(1),
		advance(30),
		...tick(1),
		{ op: 'clear' },
	], { size: 10 }),
	// The closing row has no bar on it at all — which is why `activeStyle`'s three other colours are
	// unreachable.
	bar('the closing row of a full bar has no bar on it', [
		start('Working'),
		...tick(1),
		advance(100),
		...tick(1),
		stop('Done'),
	], { size: 10 }),
	bar('in CI a bar writes one row per advance', [
		start('Working'),
		...tick(2),
		advance(50),
		...tick(2),
		advance(50),
		...tick(2),
		stop('Done'),
	], { ci: true, size: 10 }),

	// --- a task log, which has no clock ----------------------------------------------------------
	//
	// `taskLog` is the third renderer driven by calls and the first with nothing turning underneath
	// it: every byte it writes is a reply to a call. What makes it belong here anyway is the same
	// thing the spinner has — it erases what it wrote before writing again, and it counts the rows
	// to erase from the text rather than from anything it drew. So the cases that matter are the
	// ones where that count can be wrong: a line longer than the terminal, a line of characters
	// wider than one column, a limit that drops rows, and CI, where nothing is printed and the
	// erases are written anyway.

	task('a task log opens with a bar, a title and a bar', []),
	task('a title of nothing still costs a row', [], { title: '' }),
	task('one message under the title', [say('line 0')]),
	task('a second message erases the first and prints both', [say('line 0'), say('line 1')]),
	task('a message of several lines is several rows', [say('line 0\nline 1\nline 2')]),
	// A blank row inside a task log keeps its two spaces, where a blank row inside a `log` does not.
	task('a blank line inside a message keeps its bar and its spaces', [say('a\n\nb')]),
	// The buffer is still empty afterwards, so there is nothing to print and nothing to erase.
	task('a message with nothing in it prints nothing', [say(''), say('after')]),

	// --- raw --------------------------------------------------------------------------------------

	task('two raw messages are joined into one row', [say('one', true), say('two', true)]),
	task('a raw message after a plain one still starts a row', [
		say('one'),
		say('two', true),
		say('three', true),
	]),
	task('a plain message after a raw one starts a row', [say('one', true), say('two')]),

	// --- spacing, the Guide, and the title ---------------------------------------------------------

	task('no spacing at all', [say('one'), say('two'), done('finished', true)], { spacing: 0 }),
	task('three rows of spacing, which the ending walks back over', [
		say('one'),
		say('two'),
		done('finished', true),
	], { spacing: 3 }),
	// `taskLog` takes a `withGuide` and never passes it on: its `log.message` calls leave the option
	// out, so what they read is the global. This case is the recording of that — bars everywhere.
	task('the withGuide option is taken and never read', [
		say('one'),
		say('two'),
		done('finished', true),
	], { withGuide: false }),
	// The global, which is what actually turns them off. Not the title, though: those three writes
	// do not go through `log.message` at all.
	task('with the guide off globally the title keeps its bar and the messages do not', [
		say('one'),
		say('two'),
		done('finished', true),
	], { guide: false }),
	task('with the guide off globally a blank row is still two spaces', [say('a\n\nb')], {
		guide: false,
	}),
	task('with the guide off globally a group keeps its name', [
		group('install'),
		groupSay(0, 'one'),
		failed(),
	], { guide: false }),

	// --- limit and retainLog -------------------------------------------------------------------------

	task('a limit drops the oldest rows', [say('one'), say('two'), say('three'), say('four')], {
		limit: 2,
	}),
	task('a limit of one keeps the last row only', [say('one\ntwo\nthree')], { limit: 1 }),
	// `lines.length - 0` is every line, so a limit of zero holds nothing and prints nothing.
	task('a limit of zero holds nothing', [say('one'), say('two')], { limit: 0 }),
	task('a retained log keeps what the limit dropped and shows it at the end', [
		say('one'),
		say('two'),
		say('three'),
		done('finished', true),
	], { limit: 1, retainLog: true }),
	task('a retained log that is not shown is dropped with the rest', [
		say('one'),
		say('two'),
		done(),
	], { limit: 1, retainLog: true }),

	// --- groups -----------------------------------------------------------------------------------

	task('a group is printed under its name in bold', [group('install'), groupSay(0, 'one')]),
	task('a group and the log it belongs to', [
		say('starting'),
		group('install'),
		groupSay(0, 'one'),
		say('and back to the top'),
	]),
	task('two groups, each with its own rows', [
		group('install'),
		groupSay(0, 'one'),
		group('build'),
		groupSay(1, 'two'),
		groupSay(0, 'three'),
	]),
	task('a group that succeeds becomes one line', [
		group('install'),
		groupSay(0, 'one'),
		groupSay(0, 'two'),
		{ op: 'group-success', group: 0, message: 'installed' },
	]),
	task('a group that fails becomes one line, and the rest stays', [
		group('install'),
		groupSay(0, 'one'),
		{ op: 'group-error', group: 0, message: 'install failed' },
		say('after'),
	]),
	// A group with a name and no rows is skipped by `printBuffers` and printed by `renderBuffer`.
	task('an empty group is printed only at the end', [
		group('install'),
		say('one'),
		done('finished', true),
	]),

	// --- the two endings ------------------------------------------------------------------------

	task('a success hides the log', [say('one'), say('two'), done()]),
	task('a success shows it when asked', [say('one'), say('two'), done('finished', true)]),
	task('an error shows the log', [say('one'), say('two'), failed()]),
	task('an error hides it when asked', [say('one'), say('two'), failed('broken', false)]),
	task('an ending empties the log, so the next message stands alone', [
		say('one'),
		done(),
		say('two'),
		say('three'),
	]),
	// The groups go with the ending, so what comes after it is the first buffer alone.
	task('an ending drops its groups and the log carries on', [
		group('install'),
		groupSay(0, 'one'),
		say('two'),
		done(),
		say('three'),
		say('four'),
	]),
	task('an ending with a group in it', [
		group('install'),
		groupSay(0, 'one'),
		say('two'),
		failed(),
	]),

	// --- the row count, which is a string length ---------------------------------------------------
	//
	// `Math.ceil((line.length + barSize) / columns)`. Every case here is one where that is not the
	// number of rows the terminal used.

	task('a line longer than the terminal is counted and erased as two rows', [
		say('x'.repeat(30)),
		say('after'),
	], {}, 20),
	// Seventeen characters and three for the bar is exactly one row of twenty; eighteen is two.
	task('a line that fills the row exactly', [say('x'.repeat(17)), say('after')], {}, 20),
	task('a line one character over', [say('x'.repeat(18)), say('after')], {}, 20),
	// Wide characters: ten of them are ten `length` and twenty columns, so the count says one row
	// and the terminal used two.
	task('a line of wide characters is counted as half the rows it took', [
		say('ばんは'.repeat(4)),
		say('after'),
	], {}, 20),
	// An astral character is two UTF-16 units and two columns, so here the two agree by accident.
	task('a line of emoji is counted right for the wrong reason', [
		say('🎉'.repeat(8)),
		say('after'),
	], {}, 20),
	// A completed group is counted by the line it became and not by the rows it had, which is only
	// visible where the two are different heights.
	task('a group whose ending is taller than the rows it replaced', [
		group('install'),
		groupSay(0, 'one'),
		{ op: 'group-error', group: 0, message: `install failed: ${'x'.repeat(30)}` },
		say('after'),
	], {}, 20),
	// `line === '' ? 1 : …` is the one branch that a terminal three columns wide or more never
	// reaches: three for the bar already rounds up to a row. This is the terminal that reaches it.
	task('a blank row on a terminal two columns wide', [say('a\n\nb'), say('c')], {}, 2),
	task('a long line at the end of a run is erased by the ending too', [
		say('x'.repeat(30)),
		done('finished', true),
	], {}, 20),

	// --- what a message may contain ----------------------------------------------------------------

	task('a cursor escape is taken out of a message', [
		say(`one${ESC}[2Atwo`),
		say('after'),
	]),
	task('an erase escape is taken out of a message', [say(`one${ESC}[2Jtwo`)]),
	task('a save and a restore are taken out too', [say(`${ESC}[sone${ESC}[u`)]),
	// An escape `stripDestructiveANSI` does not take out stays in the message, and a Frame carries
	// no escapes (ADR-0011) — so that one shape is the module's one divergence and has no case here.
	// See `clark_core::task_log`.

	// --- CI, where nothing is drawn and everything is erased ------------------------------------------

	task('in CI the messages are not printed and the erases are written anyway', [
		say('one'),
		say('two'),
		say('three'),
		done('finished', true),
	], { ci: true }),
	task('in CI an error still prints its log', [say('one'), say('two'), failed()], { ci: true }),
	// Not CI, but not a terminal either — a run whose output is a pipe.
	task('a log written to something that is not a terminal', [say('one'), say('two'), done()], {
		tty: false,
	}),
];
