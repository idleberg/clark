//! clark's prompts, for shell scripts.
//!
//! ```sh
//! album=$(clark text "Which album?" --placeholder "Body Riddle") || exit
//! clark confirm "Play it again?" && mpv "$album"
//! ```
//!
//! Every sub-command is one Prompt, one static renderer, or one of the two that wrap a command,
//! named as clack names it, with flags named after clack's options. The answer goes to stdout; the
//! Prompt itself is drawn on stderr, so `$(...)` captures the answer and nothing else.
//!
//! Exit codes: `0` an answer, `1` a `confirm` answered no, `2` a failure, `130` a cancel — and for
//! `spinner` and `task-log`, whatever the command they wrapped exited with. See ADR-0036.

use std::io::{self, BufRead, IsTerminal, Write};
use std::process::{self, Child, ExitCode, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
use clark::{CivilDate, ClackError, DateFormat};

#[derive(Parser)]
#[command(name = "clark", version, about = "clack's prompts, from the shell")]
struct Cli {
	#[command(subcommand)]
	command: Command,
}

#[derive(Subcommand)]
enum Command {
	/// One line of text.
	Text {
		message: String,
		#[arg(long)]
		placeholder: Option<String>,
		#[arg(long)]
		initial_value: Option<String>,
		#[arg(long)]
		default_value: Option<String>,
	},

	/// Text drawn as a mask.
	Password {
		message: String,
		#[arg(long)]
		mask: Option<String>,
		#[arg(long, default_value_t = false, action = clap::ArgAction::Set)]
		clear_on_error: bool,
	},

	/// Yes or no. Exits 0 for yes, 1 for no, and prints nothing.
	Confirm {
		message: String,
		#[arg(long)]
		active: Option<String>,
		#[arg(long)]
		inactive: Option<String>,
		#[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
		initial_value: bool,
		#[arg(long, default_value_t = false, action = clap::ArgAction::Set)]
		vertical: bool,
	},

	/// One option out of a list.
	Select {
		message: String,
		#[command(flatten)]
		options: Options,
		#[arg(long)]
		initial_value: Option<String>,
		#[arg(long)]
		max_items: Option<usize>,
	},

	/// Any number of options out of a list, one per line on stdout.
	Multiselect {
		message: String,
		#[command(flatten)]
		options: Options,
		/// Repeatable: the options ticked to begin with.
		#[arg(long = "initial-value")]
		initial_values: Vec<String>,
		#[arg(long)]
		cursor_at: Option<String>,
		#[arg(long)]
		max_items: Option<usize>,
		#[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
		required: bool,
	},

	/// A multiselect under headings. Repeat `--group 'Heading:one,two'`.
	GroupMultiselect {
		message: String,
		#[arg(long = "group", required = true, value_parser = group)]
		groups: Vec<(String, Vec<String>)>,
		#[arg(long = "initial-value")]
		initial_values: Vec<String>,
		#[arg(long)]
		cursor_at: Option<String>,
		#[arg(long)]
		max_items: Option<usize>,
		#[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
		required: bool,
		#[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
		selectable_groups: bool,
	},

	/// One option, settled by the first matching keypress.
	SelectKey {
		message: String,
		#[command(flatten)]
		options: Options,
		#[arg(long)]
		initial_value: Option<String>,
		#[arg(long, default_value_t = false, action = clap::ArgAction::Set)]
		case_sensitive: bool,
	},

	/// A select whose list narrows as you type.
	Autocomplete {
		message: String,
		#[command(flatten)]
		options: Options,
		#[arg(long)]
		placeholder: Option<String>,
		#[arg(long)]
		initial_value: Option<String>,
		#[arg(long)]
		max_items: Option<usize>,
	},

	/// The same, keeping the ticks. One value per line on stdout.
	AutocompleteMultiselect {
		message: String,
		#[command(flatten)]
		options: Options,
		#[arg(long)]
		placeholder: Option<String>,
		#[arg(long = "initial-value")]
		initial_values: Vec<String>,
		#[arg(long)]
		max_items: Option<usize>,
		#[arg(long, default_value_t = false, action = clap::ArgAction::Set)]
		required: bool,
	},

	/// A path, completed against the filesystem.
	Path {
		message: String,
		#[arg(long)]
		root: Option<String>,
		#[arg(long)]
		initial_value: Option<String>,
		#[arg(long, default_value_t = false, action = clap::ArgAction::Set)]
		directory: bool,
	},

	/// A date, printed back as `YYYY-MM-DD`.
	Date {
		message: String,
		#[arg(long, value_enum, default_value_t = Format::Ymd)]
		format: Format,
		#[arg(long)]
		separator: Option<String>,
		#[arg(long, value_parser = iso)]
		initial_value: Option<CivilDate>,
		#[arg(long, value_parser = iso)]
		default_value: Option<CivilDate>,
		#[arg(long, value_parser = iso)]
		min_date: Option<CivilDate>,
		#[arg(long, value_parser = iso)]
		max_date: Option<CivilDate>,
	},

	/// Text over several lines.
	Multiline {
		message: String,
		#[arg(long)]
		placeholder: Option<String>,
		#[arg(long)]
		initial_value: Option<String>,
		#[arg(long)]
		default_value: Option<String>,
		#[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
		show_submit: bool,
	},

	/// The opening line of a run.
	Intro { title: String },
	/// The closing one.
	Outro { message: String },
	/// The other way a run ends.
	Cancel { message: String },
	/// A box beside the Guide.
	Note {
		message: String,
		#[arg(long)]
		title: Option<String>,
	},
	/// A box standing on its own.
	#[command(name = "box")]
	Box {
		message: String,
		#[arg(long)]
		title: Option<String>,
	},
	/// One line beside the Guide.
	Log {
		message: String,
		#[arg(long, value_enum, default_value_t = Level::Message)]
		level: Level,
	},

	/// A spinner, for as long as a command runs. `clark spinner "Building" -- make`
	Spinner {
		message: String,
		#[arg(long, value_enum, default_value_t = Indicator::Dots)]
		indicator: Indicator,
		/// Repeatable: the symbols cycled through.
		#[arg(long = "frame")]
		frames: Vec<String>,
		/// How long a frame is on screen, in milliseconds.
		#[arg(long)]
		delay: Option<u64>,
		/// The line left behind when the command succeeds. Defaults to the message.
		#[arg(long)]
		stop_message: Option<String>,
		/// The line left behind when it fails. Defaults to the message.
		#[arg(long)]
		error_message: Option<String>,
		/// The line left behind on `ctrl+c`. Defaults to the message.
		#[arg(long)]
		cancel_message: Option<String>,
		/// Print what the command wrote to stderr, when it fails.
		#[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
		show_error: bool,
		#[arg(last = true, required = true)]
		command: Vec<String>,
	},

	/// A command's output, cleared when it succeeds and kept when it fails.
	TaskLog {
		title: String,
		/// The most rows on screen at once. Unbounded by default, as clack leaves it.
		#[arg(long)]
		limit: Option<usize>,
		#[arg(long)]
		spacing: Option<usize>,
		/// Keep the rows `--limit` drops, and print them with the rest.
		#[arg(long, default_value_t = false, action = clap::ArgAction::Set)]
		retain_log: bool,
		/// The line left behind when the command succeeds. Defaults to the title.
		#[arg(long)]
		stop_message: Option<String>,
		/// The line left behind when it fails. Defaults to the title.
		#[arg(long)]
		error_message: Option<String>,
		/// Whether the log is kept. Left out, clack's own answer: no when the command succeeds,
		/// yes when it fails.
		#[arg(long)]
		show_log: Option<bool>,
		#[arg(last = true, required = true)]
		command: Vec<String>,
	},
}

/// The list a select-shaped Prompt offers.
///
/// Repeat `--option`, or pipe one per line — the two are the same list, so a script can build it
/// with `ls` as readily as by hand.
#[derive(clap::Args)]
struct Options {
	#[arg(long = "option")]
	option: Vec<String>,
}

impl Options {
	fn into_vec(self) -> Result<Vec<String>, Failure> {
		if !self.option.is_empty() {
			return Ok(self.option);
		}

		if io::stdin().is_terminal() {
			return Err(Failure::Message(
				"no options: pass --option, or pipe one per line".into(),
			));
		}

		let options: Vec<String> = io::stdin()
			.lock()
			.lines()
			.map_while(Result::ok)
			.filter(|line| !line.trim().is_empty())
			.collect();

		if options.is_empty() {
			Err(Failure::Message("no options on stdin".into()))
		} else {
			Ok(options)
		}
	}
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
	Ymd,
	Mdy,
	Dmy,
}

#[derive(Clone, Copy, ValueEnum)]
enum Indicator {
	Dots,
	Timer,
}

impl From<Indicator> for clark::Indicator {
	fn from(indicator: Indicator) -> Self {
		match indicator {
			Indicator::Dots => Self::Dots,
			Indicator::Timer => Self::Timer,
		}
	}
}

#[derive(Clone, Copy, ValueEnum)]
enum Level {
	Message,
	Info,
	Success,
	Step,
	Warn,
	Error,
}

/// How a sub-command ends without an answer. A Cancel is kept apart from a failure for the same
/// reason the library keeps them apart: the script may legitimately carry on after one.
enum Failure {
	Cancelled,
	Message(String),
}

impl From<ClackError> for Failure {
	fn from(error: ClackError) -> Self {
		match error {
			ClackError::Cancelled => Self::Cancelled,
			error => Self::Message(error.to_string()),
		}
	}
}

fn main() -> ExitCode {
	match run(Cli::parse().command) {
		Ok(code) => code,
		// 130 is what a shell reports for an interrupted program, which is what `ctrl+c` left.
		Err(Failure::Cancelled) => ExitCode::from(130),
		Err(Failure::Message(message)) => {
			eprintln!("clark: {message}");
			ExitCode::from(2)
		}
	}
}

fn run(command: Command) -> Result<ExitCode, Failure> {
	match command {
		Command::Text {
			message,
			placeholder,
			initial_value,
			default_value,
		} => {
			let mut prompt = clark::text(message);
			prompt = set(prompt, placeholder, clark::Text::placeholder);
			prompt = set(prompt, initial_value, clark::Text::initial_value);
			prompt = set(prompt, default_value, clark::Text::default_value);
			answer(prompt.interact()?)
		}

		Command::Password {
			message,
			mask,
			clear_on_error,
		} => {
			let mut prompt = clark::password(message).clear_on_error(clear_on_error);
			prompt = set(prompt, mask, clark::Password::mask);
			answer(prompt.interact()?)
		}

		Command::Confirm {
			message,
			active,
			inactive,
			initial_value,
			vertical,
		} => {
			let mut prompt = clark::confirm(message)
				.initial_value(initial_value)
				.vertical(vertical);
			prompt = set(prompt, active, clark::Confirm::active);
			prompt = set(prompt, inactive, clark::Confirm::inactive);

			Ok(if prompt.interact()? {
				ExitCode::SUCCESS
			} else {
				ExitCode::FAILURE
			})
		}

		Command::Select {
			message,
			options,
			initial_value,
			max_items,
		} => {
			let mut prompt = clark::select::<String>(message).options(choices(options)?);
			prompt = set(prompt, initial_value, clark::Select::initial_value);
			prompt = set(prompt, max_items, clark::Select::max_items);
			answer(prompt.interact()?)
		}

		Command::Multiselect {
			message,
			options,
			initial_values,
			cursor_at,
			max_items,
			required,
		} => {
			let mut prompt = clark::multiselect::<String>(message)
				.options(choices(options)?)
				.initial_values(initial_values)
				.required(required);
			prompt = set(prompt, cursor_at, clark::MultiSelect::cursor_at);
			prompt = set(prompt, max_items, clark::MultiSelect::max_items);
			answers(prompt.interact()?)
		}

		Command::GroupMultiselect {
			message,
			groups,
			initial_values,
			cursor_at,
			max_items,
			required,
			selectable_groups,
		} => {
			let mut prompt = clark::group_multiselect::<String>(message)
				.initial_values(initial_values)
				.required(required)
				.selectable_groups(selectable_groups);
			for (heading, values) in groups {
				prompt = prompt.group(heading, values);
			}
			prompt = set(prompt, cursor_at, clark::GroupMultiSelect::cursor_at);
			prompt = set(prompt, max_items, clark::GroupMultiSelect::max_items);
			answers(prompt.interact()?)
		}

		Command::SelectKey {
			message,
			options,
			initial_value,
			case_sensitive,
		} => {
			let mut prompt = clark::select_key::<String>(message)
				.options(choices(options)?)
				.case_sensitive(case_sensitive);
			prompt = set(prompt, initial_value, clark::SelectKey::initial_value);
			answer(prompt.interact()?)
		}

		Command::Autocomplete {
			message,
			options,
			placeholder,
			initial_value,
			max_items,
		} => {
			let mut prompt = clark::autocomplete::<String>(message).options(choices(options)?);
			prompt = set(prompt, placeholder, clark::Autocomplete::placeholder);
			prompt = set(prompt, initial_value, clark::Autocomplete::initial_value);
			prompt = set(prompt, max_items, clark::Autocomplete::max_items);
			answer(prompt.interact()?)
		}

		Command::AutocompleteMultiselect {
			message,
			options,
			placeholder,
			initial_values,
			max_items,
			required,
		} => {
			let mut prompt = clark::autocomplete_multiselect::<String>(message)
				.options(choices(options)?)
				.initial_values(initial_values)
				.required(required);
			prompt = set(
				prompt,
				placeholder,
				clark::AutocompleteMultiSelect::placeholder,
			);
			prompt = set(prompt, max_items, clark::AutocompleteMultiSelect::max_items);
			answers(prompt.interact()?)
		}

		Command::Path {
			message,
			root,
			initial_value,
			directory,
		} => {
			let mut prompt = clark::path(message).directory(directory);
			prompt = set(prompt, root, clark::Path::root);
			prompt = set(prompt, initial_value, clark::Path::initial_value);
			answer(prompt.interact()?)
		}

		Command::Date {
			message,
			format,
			separator,
			initial_value,
			default_value,
			min_date,
			max_date,
		} => {
			let mut prompt = clark::date(message).format(match format {
				Format::Ymd => DateFormat::Ymd,
				Format::Mdy => DateFormat::Mdy,
				Format::Dmy => DateFormat::Dmy,
			});
			prompt = set(prompt, separator, clark::Date::separator);
			prompt = set(prompt, initial_value, clark::Date::initial_value);
			prompt = set(prompt, default_value, clark::Date::default_value);
			prompt = set(prompt, min_date, clark::Date::min_date);
			prompt = set(prompt, max_date, clark::Date::max_date);
			answer(prompt.interact()?.iso())
		}

		Command::Multiline {
			message,
			placeholder,
			initial_value,
			default_value,
			show_submit,
		} => {
			let mut prompt = clark::multiline(message).show_submit(show_submit);
			prompt = set(prompt, placeholder, clark::MultiLine::placeholder);
			prompt = set(prompt, initial_value, clark::MultiLine::initial_value);
			prompt = set(prompt, default_value, clark::MultiLine::default_value);
			answer(prompt.interact()?)
		}

		Command::Intro { title } => still(|| clark::intro(title)),
		Command::Outro { message } => still(|| clark::outro(message)),
		Command::Cancel { message } => still(|| clark::cancel(message)),
		Command::Note { message, title } => {
			still(|| clark::note(message, title.unwrap_or_default()))
		}
		Command::Box { message, title } => {
			still(|| clark::r#box(message, title.unwrap_or_default()))
		}
		Command::Log { message, level } => still(|| match level {
			Level::Message => clark::log::message(message),
			Level::Info => clark::log::info(message),
			Level::Success => clark::log::success(message),
			Level::Step => clark::log::step(message),
			Level::Warn => clark::log::warn(message),
			Level::Error => clark::log::error(message),
		}),

		Command::Spinner {
			message,
			indicator,
			frames,
			delay,
			stop_message,
			error_message,
			cancel_message,
			show_error,
			command,
		} => {
			let mut builder = clark::spinner()
				.output(clark::Output::Stderr)
				.indicator(indicator.into());
			if !frames.is_empty() {
				builder = builder.frames(frames);
			}
			builder = set(builder, delay, |builder, delay| {
				builder.delay(Duration::from_millis(delay))
			});

			// Spawned before the spinner starts, so that a command which cannot run at all leaves
			// no row behind — it is a bad argument, and reads like every other one.
			//
			// The child's stdout is thrown away rather than passed through: it would land inside
			// the region the next frame erases. `task-log` is the sub-command that draws output.
			let child = spawn(
				&command,
				Stdio::null(),
				if show_error {
					Stdio::piped()
				} else {
					Stdio::null()
				},
			)?;

			let spinner = builder.start(&message);
			let output = child
				.wait_with_output()
				.map_err(|error| Failure::Message(format!("{}: {error}", command[0])))?;

			match output.status.code() {
				Some(0) => {
					spinner.stop(stop_message.unwrap_or(message));
					Ok(ExitCode::SUCCESS)
				}
				Some(code) => {
					spinner.error(error_message.unwrap_or(message));
					// ponytail: the replayed rows are raw, not bar-prefixed and dimmed. Styling
					// them wants `output` in `message.rs` too — and a caller who wants the output
					// drawn wants `task-log`, which draws it while the command is still running.
					if show_error {
						io::stderr().write_all(&output.stderr).ok();
					}
					Ok(ExitCode::from(exit_code(code)))
				}
				// No code means a signal killed it. `ctrl+c` reaches the whole process group, so
				// this is the cancel — drawn here rather than through `Failure::Cancelled`,
				// because that arm exits without leaving a row behind.
				None => {
					spinner.cancel(cancel_message.unwrap_or(message));
					Ok(ExitCode::from(130))
				}
			}
		}

		Command::TaskLog {
			title,
			limit,
			spacing,
			retain_log,
			stop_message,
			error_message,
			show_log,
			command,
		} => {
			let mut builder = clark::task_log(&title)
				.output(clark::Output::Stderr)
				.retain_log(retain_log);
			builder = set(builder, limit, |builder, limit| builder.limit(limit));
			builder = set(builder, spacing, |builder, spacing| {
				builder.spacing(spacing)
			});

			let mut child = spawn(&command, Stdio::piped(), Stdio::piped())?;
			let log = Arc::new(Mutex::new(builder.start()));

			// ponytail: one reader per stream, so a row's stream is ordered but the two are only
			// interleaved as the threads happen to wake. One pipe shared by both fds would order
			// them exactly, and needs `os_pipe` to duplicate the descriptor before the spawn.
			let readers: Vec<_> = [
				child.stdout.take().map(Pipe::Out),
				child.stderr.take().map(Pipe::Err),
			]
			.into_iter()
			.flatten()
			.map(|pipe| {
				let log = Arc::clone(&log);
				std::thread::spawn(move || {
					let rows: Box<dyn BufRead> = match pipe {
						Pipe::Out(out) => Box::new(io::BufReader::new(out)),
						Pipe::Err(err) => Box::new(io::BufReader::new(err)),
					};
					for row in rows.lines().map_while(Result::ok) {
						if let Ok(mut log) = log.lock() {
							log.message(row);
						}
					}
				})
			})
			.collect();

			let status = child
				.wait()
				.map_err(|error| Failure::Message(format!("{}: {error}", command[0])))?;
			for reader in readers {
				let _ = reader.join();
			}

			let mut log = log.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

			match status.code() {
				Some(0) => {
					log.success_with(stop_message.unwrap_or(title), show_log.unwrap_or(false));
					Ok(ExitCode::SUCCESS)
				}
				Some(code) => {
					log.error_with(error_message.unwrap_or(title), show_log.unwrap_or(true));
					Ok(ExitCode::from(exit_code(code)))
				}
				None => {
					log.error_with(error_message.unwrap_or(title), show_log.unwrap_or(true));
					Ok(ExitCode::from(130))
				}
			}
		}
	}
}

/// Which stream a `task-log` reader is walking. `ChildStdout` and `ChildStderr` are different
/// types, and one thread body reads either.
enum Pipe {
	Out(std::process::ChildStdout),
	Err(std::process::ChildStderr),
}

/// The wrapped command. `command` is never empty — clap requires it.
fn spawn(command: &[String], stdout: Stdio, stderr: Stdio) -> Result<Child, Failure> {
	process::Command::new(&command[0])
		.args(&command[1..])
		.stdin(Stdio::inherit())
		.stdout(stdout)
		.stderr(stderr)
		.spawn()
		.map_err(|error| Failure::Message(format!("{}: {error}", command[0])))
}

/// A wrapped command's exit code, passed through. `ExitCode` is a byte, and a code outside that is
/// reported as a plain failure rather than wrapped around into something else's meaning.
fn exit_code(code: i32) -> u8 {
	u8::try_from(code).unwrap_or(2)
}

/// Apply a builder method only when the flag was given, so that each Prompt keeps clack's own
/// default for every flag a script left out.
fn set<B, T>(builder: B, value: Option<T>, with: impl FnOnce(B, T) -> B) -> B {
	match value {
		Some(value) => with(builder, value),
		None => builder,
	}
}

fn choices(options: Options) -> Result<Vec<clark::SelectOption<String>>, Failure> {
	Ok(options
		.into_vec()?
		.into_iter()
		.map(clark::SelectOption::new)
		.collect())
}

fn answer(value: String) -> Result<ExitCode, Failure> {
	println!("{value}");
	Ok(ExitCode::SUCCESS)
}

/// One value per line, so that the answer stays something `while read` can walk.
fn answers(values: Vec<String>) -> Result<ExitCode, Failure> {
	for value in values {
		println!("{value}");
	}
	Ok(ExitCode::SUCCESS)
}

fn still(render: impl FnOnce()) -> Result<ExitCode, Failure> {
	render();
	Ok(ExitCode::SUCCESS)
}

/// `--group 'Heading:one,two'` — the shell has no objects, so a group is one string.
fn group(value: &str) -> Result<(String, Vec<String>), String> {
	let (heading, options) = value
		.split_once(':')
		.ok_or_else(|| format!("expected 'heading:one,two', got '{value}'"))?;

	let options: Vec<String> = options
		.split(',')
		.map(str::trim)
		.filter(|option| !option.is_empty())
		.map(str::to_owned)
		.collect();

	if options.is_empty() {
		return Err(format!("group '{heading}' has no options"));
	}

	Ok((heading.to_owned(), options))
}

/// `YYYY-MM-DD`, the one date spelling that does not depend on where the script is run.
fn iso(value: &str) -> Result<CivilDate, String> {
	let parts: Vec<&str> = value.split('-').collect();
	let [year, month, day] = parts[..] else {
		return Err(format!("expected YYYY-MM-DD, got '{value}'"));
	};

	let number = |part: &str| part.parse::<i64>().ok();
	number(year)
		.zip(number(month))
		.zip(number(day))
		.and_then(|((year, month), day)| CivilDate::new(year, month, day))
		.ok_or_else(|| format!("not a date: '{value}'"))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_group_is_a_heading_and_its_options() {
		assert_eq!(
			group("Body Riddle:Herr Bar, Ted").expect("a well-formed group"),
			(
				"Body Riddle".to_owned(),
				vec!["Herr Bar".to_owned(), "Ted".to_owned()]
			)
		);
		assert!(group("Body Riddle").is_err());
		assert!(group("Body Riddle:").is_err());
	}

	#[test]
	fn a_date_is_read_only_in_one_spelling() {
		assert_eq!(iso("2006-10-01").expect("a real date").iso(), "2006-10-01");
		assert!(iso("01/10/2006").is_err());
		assert!(iso("2006-13-01").is_err());
	}

	#[test]
	fn the_parser_agrees_with_itself() {
		use clap::CommandFactory;
		Cli::command().debug_assert();
	}
}
