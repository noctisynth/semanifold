pub fn init() {
    let Some(locale) = sys_locale::get_locale() else {
        rust_i18n::set_locale("en");
        return;
    };
    rust_i18n::set_locale(&locale);
}
