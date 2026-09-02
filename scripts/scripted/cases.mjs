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
];
