use colored::Colorize;

fn main() {
    if let Some(locale) = sys_locale::get_locale() {
        rust_i18n::set_locale(&locale);
    }

    if let Err(e) = semifold::run() {
        log::error!("{}", e.to_string().red());
        std::process::exit(1);
    }
}
