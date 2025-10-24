use colored::Colorize;

pub mod cli;
pub mod logger;
pub mod run;
pub mod utils;

rust_i18n::i18n!("locales", fallback = "en");

fn main() {
    if let Some(locale) = sys_locale::get_locale() {
        rust_i18n::set_locale(&locale);
    }

    if let Err(e) = run::run() {
        log::error!("{}", e.to_string().red());
        std::process::exit(1);
    }
}
