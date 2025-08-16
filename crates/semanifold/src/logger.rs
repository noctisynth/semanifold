use colored::{ColoredString, Colorize};
pub use log::LevelFilter;
pub use log::{debug, error, info, trace, warn};

fn format_level(level: log::Level) -> ColoredString {
    match level {
        log::Level::Trace => level.as_str().magenta(),
        log::Level::Debug => level.as_str().blue(),
        log::Level::Info => level.as_str().green(),
        log::Level::Warn => level.as_str().yellow(),
        log::Level::Error => level.as_str().red(),
    }
}

pub fn setup_logger(level: log::LevelFilter) -> Result<(), fern::InitError> {
    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{:<5} {} {}",
                format_level(record.level()),
                record.target().cyan(),
                message
            ))
        })
        .level(level)
        .chain(std::io::stdout())
        .apply()?;
    Ok(())
}
