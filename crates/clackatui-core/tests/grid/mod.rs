//! The emulator harness both parity tests are read through.
//!
//! ADR-0001 puts the compatibility claim on the Grid — characters, styles and cursor position —
//! rather than on the bytes, because clack's Frames are picocolors output and this crate's are
//! `Style`s stated per run (ADR-0011). Two streams that encode the same appearance differently
//! agree here and nowhere else.
//!
//! Shared rather than copied, because the arbiter of every appearance claim in the project should
//! be one thing: a Grid built two slightly different ways in two files would be two claims wearing
//! one name.

// Two test binaries include this, and neither uses all of it: a Scenario resizes part-way through
// and builds its Grid segment by segment, a static renderer writes once. Both halves are used.
#![allow(dead_code)]

use avt::{Cell, Vt};

/// The observable terminal state: characters, styles and cursor position.
///
/// Deliberately the whole terminal rather than the rows the output happens to occupy. Where writing
/// *stops* is part of what is being checked — a port that erased one row too few would leave a
/// difference below its own output, and cropping would hide it.
#[derive(PartialEq)]
pub struct Grid {
	pub rows: Vec<Vec<Cell>>,
	pub cursor: (usize, usize),
	pub cursor_visible: bool,
}

impl Grid {
	/// Everything on the terminal, as text. For emptiness guards and failure messages, not for the
	/// comparison — a Grid is more than its characters.
	pub fn text(&self) -> String {
		self.rows
			.iter()
			.map(|cells| characters(cells))
			.collect::<Vec<_>>()
			.join("\n")
	}

	/// What a terminal of this size holds after `bytes`. The single-segment case; a stream that
	/// changes the terminal's size part-way through needs [`feed`] and [`read`] instead.
	pub fn of(bytes: &str, columns: usize, rows: usize) -> Self {
		let mut vt = Vt::new(columns, rows);
		feed(&mut vt, bytes);
		read(&vt)
	}
}

/// Write `bytes` to `vt`, through the line discipline clack's output goes through.
///
/// `ONLCR`: clack writes to `process.stdout`, which is a tty in normal output mode — a Prompt puts
/// the terminal's *input* into raw mode and leaves the output alone — so every `\n` returns the
/// cursor to the first column before moving down. `avt` is the emulator and not the line
/// discipline, so it has to be told. Without this the second row of every Frame starts wherever the
/// first ended, and a Prompt that draws six rows draws them down a staircase no terminal has ever
/// shown anyone.
///
/// It is not cosmetic. A staircase leaves cells to the left of each row that nothing ever wrote, and
/// `avt` pads them with whatever style was open at the time — so the comparison was reading a
/// difference in the escape *before* a newline off cells that do not exist on a real terminal. That
/// is also the answer to why `ESC[999D` is written before every cursor walk: after a Frame whose
/// last row has no newline after it the cursor is at the end of that row, and clack has to get back
/// to the first column to count rows.
pub fn feed(vt: &mut Vt, bytes: &str) {
	vt.feed_str(&bytes.replace('\n', "\r\n"));
}

/// The Grid a terminal is currently showing.
pub fn read(vt: &Vt) -> Grid {
	let cursor = vt.cursor();
	Grid {
		rows: vt.view().map(|line| line.cells().to_vec()).collect(),
		cursor: (cursor.col, cursor.row),
		cursor_visible: cursor.visible,
	}
}

/// Two Grids, with the parts that agree left out.
///
/// A failure here is read by someone who has to work out which write caused it, so it reports the
/// rows that differ and how, rather than two screens to be diffed by eye.
pub fn difference(theirs: &Grid, ours: &Grid) -> String {
	let mut out = Vec::new();

	if theirs.cursor != ours.cursor {
		out.push(format!(
			"     cursor: clack left it at {:?}, the port at {:?}",
			theirs.cursor, ours.cursor
		));
	}
	if theirs.cursor_visible != ours.cursor_visible {
		out.push(format!(
			"     cursor: clack left it {}, the port {}",
			shown(theirs.cursor_visible),
			shown(ours.cursor_visible),
		));
	}

	for (index, (theirs, ours)) in theirs.rows.iter().zip(&ours.rows).enumerate() {
		if theirs == ours {
			continue;
		}
		out.push(format!("     row {index}"));
		out.push(format!("       clack: {}", row(theirs)));
		out.push(format!("        port: {}", row(ours)));

		// Where the two rows say the same thing but wear it differently, the text above is
		// identical and useless. Name the first cell that differs instead.
		if row(theirs) == row(ours) {
			if let Some((column, theirs, ours)) = theirs
				.iter()
				.zip(ours)
				.enumerate()
				.find(|(_, (a, b))| a != b)
				.map(|(column, (a, b))| (column, a, b))
			{
				out.push(format!(
					"       column {column} is styled {:?} by clack and {:?} by the port",
					theirs.pen(),
					ours.pen()
				));
			}
		}
	}

	out.join("\n")
}

/// The characters of a row, read once each.
///
/// A wide character occupies two cells and `avt` stores it in both, so taking `char` from every
/// cell would read it twice — which is only visible where there are wide characters, and so was
/// invisible until the hand-authored Scenarios arrived. The tail cell is the one with no width of
/// its own. The comparison itself is unaffected: it is over whole `Cell`s, occupancy included.
pub fn characters(cells: &[Cell]) -> String {
	cells
		.iter()
		.filter(|cell| cell.width() > 0)
		.map(Cell::char)
		.collect()
}

/// A row as text, with the trailing blanks cut off so that a failure fits on a line.
fn row(cells: &[Cell]) -> String {
	format!("{:?}", characters(cells).trim_end())
}

fn shown(visible: bool) -> &'static str {
	if visible { "visible" } else { "hidden" }
}
