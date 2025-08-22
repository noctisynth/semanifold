pub fn init() {
    sys_locale::get_locale().map(|locale| {
        rust_i18n::set_locale(&locale);
    });
}
