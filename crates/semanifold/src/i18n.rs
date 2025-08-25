pub fn init() {
    if let Some(locale) = sys_locale::get_locale() {
        rust_i18n::set_locale(&locale);
    }
}
