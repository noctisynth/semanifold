pub fn init() {
    let locale = sys_locale::get_locale();
    if let Some(locale) = locale {
        rust_i18n::set_locale(locale.as_str());
    }
}
