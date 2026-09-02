//! Ported from `@clack/prompts`' `path.ts` — an `autocomplete` whose list is the filesystem.
//!
//! `path` adds no widget, no state machine and no key handling. It is
//! [`crate::autocomplete`] with three things set: `maxItems` is five, `initialUserInput` is the
//! root, and `options` is a *function* rather than a list. That last one is the whole prompt, and
//! it is why this module exists at all.
//!
//! # The list is not filtered, it is re-read
//!
//! `AutocompletePrompt`'s `options` may be an array or a function, and the two are not the same
//! Prompt. When it is a function the constructor takes `opts.filter` **without a fallback**, so a
//! caller who passes no filter gets none — and `path` passes none. Every keystroke therefore calls
//! the function again and keeps everything it returns: the search is done by the filesystem, in
//! [`options`], and the filter that a plain `autocomplete` would run over the result is not there
//! to run. [`crate::autocomplete::AutocompleteState::with_options_fn`] is that shape.
//!
//! # `Fs` is the seam
//!
//! `path.ts` imports `node:fs` at the top of the file, and clack's own suite replaces it with
//! `memfs` before the module loads. Here that swap is a parameter: [`Fs`] is three questions asked
//! of a filesystem, `clackatui::path::StdFs` answers them with `std::fs`, and the Scenarios answer
//! them from the volume the recording carries. The crate keeps its rule — nothing here opens a
//! file.
//!
//! # `node:path`, not `std::path`
//!
//! `dirname` and `join` are the ones `path.ts` imports, and their answers are a specification
//! rather than an implementation detail: `dirname` is what decides which directory gets listed
//! after each keystroke. [`dirname`] and [`join`] below are ports of node's *posix* versions,
//! character for character. `std::path::Path::parent` is a different function — it answers `None`
//! where node answers `"."`, and keeps a trailing component that node's trailing-slash scan drops.
//!
//! Posix, on every platform. `path.ts` gets whichever `node:path` the platform hands it, so a
//! Windows clack separates with `\`; the corpus behind this module is posix and nothing here
//! guesses at the other one.

use crate::select::SelectOption;

/// The three questions `path.ts` asks `node:fs`.
///
/// `existsSync` and `lstatSync().isDirectory()` are separate because upstream asks them separately
/// and branches between the two calls. `read_dir` answers `None` where `readdirSync` throws, which
/// is upstream's `catch` — reachable whenever the directory the text names is not one.
pub trait Fs {
	/// `existsSync`.
	fn exists(&self, path: &str) -> bool;

	/// `lstatSync(path).isDirectory()`. False for anything that is not a directory, and for anything
	/// that cannot be stat'd — upstream would throw and land in the same empty list.
	fn is_dir(&self, path: &str) -> bool;

	/// `readdirSync`: the names in a directory, in the order the filesystem gives them. `None` is a
	/// throw.
	///
	/// Not sorted here. `readdirSync` does not sort, so neither does this; what a suggestion list is
	/// ordered by is the filesystem's business and it differs between one and the next.
	fn read_dir(&self, path: &str) -> Option<Vec<String>>;
}

/// `path`'s `options()`, which runs on every keystroke.
///
/// The text in the field is a path being typed, so the directory to list is the deepest one that
/// exists along it, and the suggestions are that directory's entries that the text is a prefix of.
/// Everything upstream wraps in one `try` is here as an early return: a directory that cannot be
/// read suggests nothing.
///
/// The trailing slash is the difference between "the directory I am in" and "the directory I have
/// named". In `directory` mode a path with no slash after it lists its *siblings*, so that enter
/// answers with the directory itself; add the slash and it lists its children.
pub fn options<F: Fs + ?Sized>(
	fs: &F,
	user_input: &str,
	directory: bool,
) -> Vec<SelectOption<String>> {
	if user_input.is_empty() {
		return Vec::new();
	}

	let search = if !fs.exists(user_input) {
		dirname(user_input)
	} else if fs.is_dir(user_input) && (!directory || user_input.ends_with('/')) {
		user_input.to_string()
	} else {
		dirname(user_input)
	};

	// "Strip trailing slash so startsWith matches the directory itself among its siblings" —
	// upstream's own comment. Length in UTF-16 units, as upstream's `.length` is, though the only
	// thing being asked is whether the text is longer than the `/` it ends with.
	let prefix = if user_input.encode_utf16().count() > 1 && user_input.ends_with('/') {
		&user_input[..user_input.len() - 1]
	} else {
		user_input
	};

	let Some(entries) = fs.read_dir(&search) else {
		return Vec::new();
	};

	entries
		.into_iter()
		.map(|entry| {
			let path = join(&search, &entry);
			let is_dir = fs.is_dir(&path);
			(path, is_dir)
		})
		.filter(|(path, is_dir)| path.starts_with(prefix) && (*is_dir || !directory))
		.map(|(path, _)| SelectOption::new(path))
		.collect()
}

/// `path`'s own validator: everything `opts.validate` says, and a path where there is none.
///
/// Written by `path()` rather than passed to it, which is why it is here — the same bargain
/// `multiselect`'s `required` is under. An empty answer is `!value` upstream, and the answer is a
/// value the list holds, so the empty one is the list having nothing to offer.
pub const REQUIRED_ERROR: &str = "Please select a path";

/// `validate` as `path` installs it, over an `autocomplete`'s selection.
pub fn required(value: Option<&Vec<String>>) -> Option<String> {
	let empty = match value {
		None => true,
		Some(values) => values.first().is_none_or(String::is_empty),
	};
	empty.then(|| REQUIRED_ERROR.to_string())
}

/// `node:path`'s posix `dirname`, ported as it is written.
///
/// The scan starts at index 1 and skips trailing slashes, so `"/a/b/"` is `"/a"` and `"a"` is
/// `"."`. The `"//"` case is node's own and is kept for the same reason every other quirk in this
/// crate is.
pub fn dirname(path: &str) -> String {
	let bytes = path.as_bytes();
	if bytes.is_empty() {
		return ".".to_string();
	}
	let has_root = bytes[0] == b'/';
	let mut end: Option<usize> = None;
	let mut matched_slash = true;
	for index in (1..bytes.len()).rev() {
		if bytes[index] == b'/' {
			if !matched_slash {
				end = Some(index);
				break;
			}
		} else {
			matched_slash = false;
		}
	}
	match end {
		None if has_root => "/".to_string(),
		None => ".".to_string(),
		Some(1) if has_root => "//".to_string(),
		Some(end) => path[..end].to_string(),
	}
}

/// `node:path`'s posix `join` of exactly two parts, which is all `path.ts` asks for.
///
/// The parts are joined with a slash and the result normalised — so `join(".", "x")` is `"x"` and
/// `join("/tmp/", "x")` is `"/tmp/x"`, both of which a plain concatenation gets wrong.
pub fn join(a: &str, b: &str) -> String {
	let joined = match (a.is_empty(), b.is_empty()) {
		(true, true) => return ".".to_string(),
		(true, false) => b.to_string(),
		(false, true) => a.to_string(),
		(false, false) => format!("{a}/{b}"),
	};
	// `join` normalises whatever it built, including a single part — which is what turns `"./x"`
	// into `"x"` and `"a//b"` into `"a/b"`.
	normalize(&joined)
}

/// `node:path`'s posix `normalize`: `.` and `..` resolved, repeated slashes collapsed, a trailing
/// slash kept if there was one.
pub fn normalize(path: &str) -> String {
	if path.is_empty() {
		return ".".to_string();
	}
	let absolute = path.starts_with('/');
	let trailing = path.ends_with('/');

	let mut parts: Vec<&str> = Vec::new();
	for segment in path.split('/') {
		match segment {
			"" | "." => {}
			".." => match parts.last() {
				Some(&last) if last != ".." => {
					parts.pop();
				}
				// Above the root is nowhere; above a relative path is `..`.
				_ if !absolute => parts.push(".."),
				_ => {}
			},
			segment => parts.push(segment),
		}
	}

	let mut out = parts.join("/");
	if out.is_empty() {
		return match (absolute, trailing) {
			(true, _) => "/".to_string(),
			(false, true) => "./".to_string(),
			(false, false) => ".".to_string(),
		};
	}
	if trailing {
		out.push('/');
	}
	if absolute {
		out.insert(0, '/');
	}
	out
}

/// A filesystem held in memory: what the Scenarios read, and what `memfs` is upstream.
///
/// Not a toy for tests only — it is the other half of the seam. Clack's own `path` suite mocks
/// `node:fs` with `memfs` and builds a volume per case, and the recordings this crate is held
/// against were made against that volume, so replaying them means answering [`Fs`] from the same
/// listing. Contents are not held: nothing in `path.ts` reads a file, only its kind.
///
/// **Listings are sorted**, because `memfs` sorts and a real filesystem does not. That difference
/// is upstream's, not the port's: it decides which suggestion is the first one, and so which one a
/// bare enter answers with.
pub struct MemFs {
	/// Every path in the volume, with `true` for a directory. Sorted, which is what makes
	/// [`Fs::read_dir`] sorted.
	entries: std::collections::BTreeMap<String, bool>,
}

impl MemFs {
	/// A volume from `(path, is_directory)` pairs — `vol.fromJSON()`, where an entry with no
	/// contents is an empty directory.
	///
	/// The directories along each path are created with it, as `fromJSON` creates them, and the root
	/// exists whether or not anything was said about it.
	pub fn new<'a>(entries: impl IntoIterator<Item = (&'a str, bool)>) -> Self {
		let mut held = std::collections::BTreeMap::new();
		held.insert("/".to_string(), true);
		for (path, is_dir) in entries {
			let path = path.trim_end_matches('/');
			held.insert(path.to_string(), is_dir);
			let mut at = path;
			while let Some(cut) = at.rfind('/') {
				at = &at[..cut];
				if at.is_empty() {
					break;
				}
				held.insert(at.to_string(), true);
			}
		}
		Self { entries: held }
	}

	fn kind(&self, path: &str) -> Option<bool> {
		let trimmed = path.trim_end_matches('/');
		self.entries
			.get(if trimmed.is_empty() { "/" } else { trimmed })
			.copied()
	}
}

impl Fs for MemFs {
	fn exists(&self, path: &str) -> bool {
		self.kind(path).is_some()
	}

	fn is_dir(&self, path: &str) -> bool {
		self.kind(path) == Some(true)
	}

	fn read_dir(&self, path: &str) -> Option<Vec<String>> {
		if !self.is_dir(path) {
			return None;
		}
		let base = format!("{}/", path.trim_end_matches('/'));
		Some(
			self.entries
				.keys()
				.filter_map(|held| {
					let rest = held.strip_prefix(&base)?;
					(!rest.is_empty() && !rest.contains('/')).then(|| rest.to_string())
				})
				.collect(),
		)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// The volume upstream's own `path` suite builds, in the order it builds it.
	fn upstream() -> MemFs {
		MemFs::new([
			("/tmp/foo/bar.txt", false),
			("/tmp/foo/baz.text", false),
			("/tmp/hello/world.jpg", false),
			("/tmp/hello/john.jpg", false),
			("/tmp/hello/jeanne.png", false),
			("/tmp/root.zip", false),
			("/tmp/bar", false),
		])
	}

	fn suggested(input: &str, directory: bool) -> Vec<String> {
		options(&upstream(), input, directory)
			.into_iter()
			.map(|option| option.value().clone())
			.collect()
	}

	#[test]
	fn an_empty_field_suggests_nothing() {
		assert!(suggested("", false).is_empty());
	}

	#[test]
	fn a_directory_with_a_slash_suggests_its_children() {
		assert_eq!(
			suggested("/tmp/", false),
			["/tmp/bar", "/tmp/foo", "/tmp/hello", "/tmp/root.zip"]
		);
	}

	/// Upstream's own reason for stripping the trailing slash: a slash typed after something that is
	/// *not* a directory sends the search to the parent, and the name has to match there without it.
	#[test]
	fn a_slash_after_a_file_still_finds_the_file() {
		assert_eq!(suggested("/tmp/bar/", false), ["/tmp/bar"]);
	}

	/// The trailing slash is stripped before the prefix test, so a directory appears among its own
	/// siblings rather than filtering them all out.
	#[test]
	fn a_directory_without_a_slash_suggests_its_children_too() {
		assert_eq!(
			suggested("/tmp", false),
			["/tmp/bar", "/tmp/foo", "/tmp/hello", "/tmp/root.zip"]
		);
	}

	#[test]
	fn a_partial_name_lists_the_parent_and_keeps_what_starts_with_it() {
		assert_eq!(suggested("/tmp/r", false), ["/tmp/root.zip"]);
		assert_eq!(suggested("/tmp/ba", false), ["/tmp/bar"]);
	}

	/// In `directory` mode a named directory lists its siblings, so that enter answers with the
	/// directory rather than jumping into it. The slash is what asks for the children.
	#[test]
	fn directory_mode_turns_on_the_trailing_slash() {
		// The siblings of `/tmp`, which under this volume is `/tmp` itself — so enter answers with
		// the directory that was typed.
		assert_eq!(suggested("/tmp", true), ["/tmp"]);
		assert_eq!(suggested("/tmp/", true), ["/tmp/foo", "/tmp/hello"]);
		assert_eq!(suggested("/tmp/f", true), ["/tmp/foo"]);
	}

	#[test]
	fn a_path_that_is_nowhere_suggests_nothing() {
		assert!(suggested("/nope/at/all", false).is_empty());
	}

	#[test]
	fn nothing_that_matches_is_nothing() {
		assert!(suggested("/tmp/_", false).is_empty());
	}

	#[test]
	fn an_empty_answer_is_the_one_thing_path_refuses() {
		assert_eq!(required(None).as_deref(), Some(REQUIRED_ERROR));
		assert_eq!(required(Some(&vec![])).as_deref(), Some(REQUIRED_ERROR));
		assert_eq!(
			required(Some(&vec![String::new()])).as_deref(),
			Some(REQUIRED_ERROR)
		);
		assert_eq!(required(Some(&vec!["/tmp/bar".to_string()])), None);
	}

	/// node's own answers, which are not `std::path`'s.
	#[test]
	fn dirname_is_nodes() {
		assert_eq!(dirname(""), ".");
		assert_eq!(dirname("a"), ".");
		assert_eq!(dirname("a/b"), "a");
		assert_eq!(dirname("a/b/"), "a");
		assert_eq!(dirname("/"), "/");
		assert_eq!(dirname("/a"), "/");
		assert_eq!(dirname("/a/"), "/");
		assert_eq!(dirname("/a/b"), "/a");
		assert_eq!(dirname("/tmp/foo/bar.txt"), "/tmp/foo");
		assert_eq!(dirname("//a"), "//");
	}

	/// `readdirSync` never hands back a `..` and `dirname` never builds one, so nothing upstream
	/// reaches these — but `normalize` is node's function and this is what node's answers are.
	#[test]
	fn normalize_is_nodes() {
		assert_eq!(normalize("/a/"), "/a/");
		assert_eq!(normalize("a//b"), "a/b");
		assert_eq!(normalize("./a"), "a");
		assert_eq!(normalize(""), ".");
		assert_eq!(normalize("/"), "/");
		// Above the root is the root; above a relative path is above it.
		assert_eq!(normalize("/a/../.."), "/");
		assert_eq!(normalize("a/../.."), "..");
	}

	#[test]
	fn a_memfs_refuses_to_list_a_file() {
		assert_eq!(upstream().read_dir("/tmp/bar"), None);
		assert!(upstream().read_dir("/tmp").is_some());
	}

	#[test]
	fn join_is_nodes() {
		assert_eq!(join("/tmp", "foo"), "/tmp/foo");
		assert_eq!(join("/tmp/", "foo"), "/tmp/foo");
		assert_eq!(join("/", "foo"), "/foo");
		assert_eq!(join(".", "foo"), "foo");
		assert_eq!(join("", "foo"), "foo");
		assert_eq!(join("a/b", ".."), "a");
		assert_eq!(join("a", "../.."), "..");
	}
}
