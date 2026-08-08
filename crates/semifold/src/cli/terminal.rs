use std::{
    io::{IsTerminal, Write},
    sync::{Mutex, OnceLock},
    time::Duration,
};

use colored::Colorize;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use unicode_width::UnicodeWidthStr;

static ACTIVE_PROGRESS: OnceLock<Mutex<Option<ProgressBar>>> = OnceLock::new();

pub(crate) struct ProgressAwareStderr;

impl Write for ProgressAwareStderr {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let active = ACTIVE_PROGRESS
            .get_or_init(|| Mutex::new(None))
            .lock()
            .ok()
            .and_then(|guard| guard.clone());
        if let Some(progress) = active {
            progress.suspend(|| std::io::stderr().write(buffer))
        } else {
            std::io::stderr().write(buffer)
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        std::io::stderr().flush()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StepOutcome {
    Success,
    Skipped,
    Failed,
}

pub(crate) struct Terminal {
    progress: ProgressBackend,
    unicode: bool,
}

enum ProgressBackend {
    Indicatif(IndicatifProgressReporter),
    Plain(PlainProgressReporter),
}

trait ProgressReporter {
    fn begin(&self, message: String) -> ProgressTask;
}

struct IndicatifProgressReporter {
    unicode: bool,
}

struct PlainProgressReporter {
    unicode: bool,
}

impl Terminal {
    pub(crate) fn detect() -> Self {
        let stderr_terminal = std::io::stderr().is_terminal();
        let ci = std::env::var("CI").is_ok() || std::env::var("GITHUB_ACTIONS").is_ok();
        let dumb = std::env::var("TERM").is_ok_and(|term| term == "dumb");
        let unicode = !dumb;
        let progress = if stderr_terminal && !ci && !dumb {
            ProgressBackend::Indicatif(IndicatifProgressReporter { unicode })
        } else {
            ProgressBackend::Plain(PlainProgressReporter { unicode })
        };
        Self { progress, unicode }
    }

    #[cfg(test)]
    pub(crate) const fn plain(unicode: bool) -> Self {
        Self {
            progress: ProgressBackend::Plain(PlainProgressReporter { unicode }),
            unicode,
        }
    }

    pub(crate) fn heading(&self, title: &str) {
        println!("{}", title.bold());
        println!();
    }

    pub(crate) fn dry_run(&self, message: &str) {
        println!("{}", message.yellow().bold());
        println!();
    }

    pub(crate) fn section(&self, title: &str) {
        println!("{}", title.bold());
    }

    pub(crate) fn fact(&self, label: &str, value: impl std::fmt::Display) {
        println!("  {} {}", Self::cell(label, 14).dimmed(), value);
    }

    pub(crate) fn line(&self, message: impl std::fmt::Display) {
        println!("{message}");
    }

    pub(crate) fn blank(&self) {
        println!();
    }

    pub(crate) fn warning(&self, message: &str) {
        let _ = writeln!(
            std::io::stderr(),
            "{} {}",
            self.symbol(StepOutcome::Skipped).yellow(),
            message.yellow()
        );
    }

    pub(crate) fn recovery(&self, title: &str, message: &str) {
        let _ = writeln!(std::io::stderr(), "\n{}", title.bold());
        let _ = writeln!(std::io::stderr(), "  {message}");
    }

    pub(crate) fn summary(&self, outcome: StepOutcome, message: &str) {
        let symbol = self.symbol(outcome);
        let rendered = match outcome {
            StepOutcome::Success => format!("{symbol} {message}").green().bold(),
            StepOutcome::Skipped => format!("{symbol} {message}").yellow().bold(),
            StepOutcome::Failed => format!("{symbol} {message}").red().bold(),
        };
        match outcome {
            StepOutcome::Failed => {
                let _ = writeln!(std::io::stderr(), "{rendered}");
            }
            StepOutcome::Success | StepOutcome::Skipped => println!("{rendered}"),
        }
    }

    pub(crate) fn progress(&self, message: impl Into<String>) -> ProgressTask {
        match &self.progress {
            ProgressBackend::Indicatif(reporter) => reporter.begin(message.into()),
            ProgressBackend::Plain(reporter) => reporter.begin(message.into()),
        }
    }

    pub(crate) fn cell(value: impl AsRef<str>, width: usize) -> String {
        let value = value.as_ref();
        let padding = width.saturating_sub(UnicodeWidthStr::width(value));
        format!("{value}{}", " ".repeat(padding))
    }

    fn symbol(&self, outcome: StepOutcome) -> &'static str {
        symbol(self.unicode, outcome)
    }
}

impl ProgressReporter for IndicatifProgressReporter {
    fn begin(&self, message: String) -> ProgressTask {
        let bar = ProgressBar::with_draw_target(None, ProgressDrawTarget::stderr_with_hz(12));
        let template = if self.unicode {
            "  {spinner:.cyan} {msg}"
        } else {
            "  {spinner} {msg}"
        };
        if let Ok(style) = ProgressStyle::with_template(template) {
            bar.set_style(if self.unicode {
                style.tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            } else {
                style.tick_strings(&["-", "\\", "|", "/"])
            });
        }
        bar.set_message(message);
        bar.enable_steady_tick(Duration::from_millis(80));
        if let Ok(mut active) = ACTIVE_PROGRESS.get_or_init(|| Mutex::new(None)).lock() {
            *active = Some(bar.clone());
        }
        ProgressTask {
            bar,
            unicode: self.unicode,
            plain: false,
        }
    }
}

impl ProgressReporter for PlainProgressReporter {
    fn begin(&self, message: String) -> ProgressTask {
        let pending = if self.unicode { "…" } else { "..." };
        let _ = writeln!(std::io::stderr(), "  {pending} {message}");
        ProgressTask {
            bar: ProgressBar::hidden(),
            unicode: self.unicode,
            plain: true,
        }
    }
}

const fn symbol(unicode: bool, outcome: StepOutcome) -> &'static str {
    match (unicode, outcome) {
        (true, StepOutcome::Success) => "✓",
        (true, StepOutcome::Skipped) => "•",
        (true, StepOutcome::Failed) => "✗",
        (false, StepOutcome::Success) => "[ok]",
        (false, StepOutcome::Skipped) => "[skip]",
        (false, StepOutcome::Failed) => "[failed]",
    }
}

pub(crate) struct ProgressTask {
    bar: ProgressBar,
    unicode: bool,
    plain: bool,
}

impl ProgressTask {
    pub(crate) fn suspend<T>(&self, operation: impl FnOnce() -> T) -> T {
        if self.plain {
            operation()
        } else {
            self.bar.suspend(operation)
        }
    }

    pub(crate) fn finish(self, outcome: StepOutcome, message: impl Into<String>) {
        let message = message.into();
        let symbol = symbol(self.unicode, outcome);
        if self.plain {
            let _ = writeln!(std::io::stderr(), "  {symbol} {message}");
        } else {
            self.bar.finish_and_clear();
            clear_active_progress();
            let rendered = match outcome {
                StepOutcome::Success => format!("  {symbol} {message}").green(),
                StepOutcome::Skipped => format!("  {symbol} {message}").yellow(),
                StepOutcome::Failed => format!("  {symbol} {message}").red(),
            };
            let _ = writeln!(std::io::stderr(), "{rendered}");
        }
    }

    #[cfg(test)]
    fn is_hidden(&self) -> bool {
        self.bar.is_hidden()
    }
}

impl Drop for ProgressTask {
    fn drop(&mut self) {
        if !self.bar.is_finished() {
            self.bar.finish_and_clear();
        }
        if !self.plain {
            clear_active_progress();
        }
    }
}

fn clear_active_progress() {
    if let Ok(mut active) = ACTIVE_PROGRESS.get_or_init(|| Mutex::new(None)).lock() {
        *active = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_progress_is_hidden_and_uses_ascii_when_requested() {
        let terminal = Terminal::plain(false);
        let progress = terminal.progress("testing");
        assert!(progress.is_hidden());
        assert_eq!(terminal.symbol(StepOutcome::Success), "[ok]");
        assert_eq!(terminal.symbol(StepOutcome::Failed), "[failed]");
        assert_eq!(Terminal::cell("目标版本", 12), "目标版本    ");
    }

    #[test]
    fn fact_labels_use_unicode_display_width() {
        assert_eq!(Terminal::cell("变更集", 14), "变更集        ");
        assert_eq!(Terminal::cell("计划指纹", 14), "计划指纹      ");
    }
}
