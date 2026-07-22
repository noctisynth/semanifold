use std::collections::BTreeSet;

use toml_edit::TableLike;

fn locale_keys(content: &str) -> BTreeSet<String> {
    let document = content.parse::<toml_edit::DocumentMut>().unwrap();
    let mut keys = BTreeSet::new();
    collect_keys(document.as_table(), "", &mut keys);
    keys
}

fn collect_keys(table: &dyn TableLike, prefix: &str, keys: &mut BTreeSet<String>) {
    for (key, item) in table.iter() {
        let path = if prefix.is_empty() {
            key.to_string()
        } else {
            format!("{prefix}.{key}")
        };
        if let Some(table) = item.as_table() {
            collect_keys(table, &path, keys);
        } else {
            keys.insert(path);
        }
    }
}

#[test]
fn chinese_locale_has_the_same_keys_as_english() {
    let english = locale_keys(include_str!("../locales/en.toml"));
    let chinese = locale_keys(include_str!("../locales/zh.toml"));

    assert_eq!(
        chinese, english,
        "zh.toml must not rely on the English fallback"
    );
}
