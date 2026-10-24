use std::fmt::Display;

use super::{Count, Tr, Value, Words};

fn tr(values: &dyn askama::Values) -> askama::Result<&Tr> {
    askama::get_value::<Tr>(values, "tr")
}

#[askama::filter_fn]
pub fn t(id: &str, values: &dyn askama::Values) -> askama::Result<Words> {
    Ok(tr(values)?.t(id))
}

/// `$count` picks the plural.
#[askama::filter_fn]
pub fn tn<C: Count>(id: &str, values: &dyn askama::Values, count: C) -> askama::Result<Words> {
    Ok(tr(values)?.args(id, &[("count", count.value())]))
}

/// One variable, set as text.
#[askama::filter_fn]
pub fn ta<V: Display>(
    id: &str,
    values: &dyn askama::Values,
    name: &str,
    value: V,
) -> askama::Result<Words> {
    Ok(tr(values)?.args(id, &[(name, Value::from(value.to_string()))]))
}

/// Two variables, set as text.
#[askama::filter_fn]
pub fn ta2<V: Display, W: Display>(
    id: &str,
    values: &dyn askama::Values,
    name: &str,
    value: V,
    name2: &str,
    value2: W,
) -> askama::Result<Words> {
    Ok(tr(values)?.args(
        id,
        &[
            (name, Value::from(value.to_string())),
            (name2, Value::from(value2.to_string())),
        ],
    ))
}

/// Two numbers, each able to pick a plural.
#[askama::filter_fn]
pub fn tn2<C: Count, D: Count>(
    id: &str,
    values: &dyn askama::Values,
    name: &str,
    count: C,
    name2: &str,
    count2: D,
) -> askama::Result<Words> {
    Ok(tr(values)?.args(id, &[(name, count.value()), (name2, count2.value())]))
}

/// Three numbers, each able to pick a plural.
#[askama::filter_fn]
pub fn tn3<C: Count, D: Count, E: Count>(
    id: &str,
    values: &dyn askama::Values,
    name: &str,
    count: C,
    name2: &str,
    count2: D,
    name3: &str,
    count3: E,
) -> askama::Result<Words> {
    Ok(tr(values)?.args(
        id,
        &[
            (name, count.value()),
            (name2, count2.value()),
            (name3, count3.value()),
        ],
    ))
}

/// `$count` picks the plural, and one more variable set as text.
#[askama::filter_fn]
pub fn tna<C: Count, V: Display>(
    id: &str,
    values: &dyn askama::Values,
    count: C,
    name: &str,
    value: V,
) -> askama::Result<Words> {
    Ok(tr(values)?.args(
        id,
        &[
            ("count", count.value()),
            (name, Value::from(value.to_string())),
        ],
    ))
}

/// `$count` picks the plural, and two more variables set as text.
#[askama::filter_fn]
pub fn tna2<C: Count, V: Display, W: Display>(
    id: &str,
    values: &dyn askama::Values,
    count: C,
    name: &str,
    value: V,
    name2: &str,
    value2: W,
) -> askama::Result<Words> {
    Ok(tr(values)?.args(
        id,
        &[
            ("count", count.value()),
            (name, Value::from(value.to_string())),
            (name2, Value::from(value2.to_string())),
        ],
    ))
}

/// The words a page's scripts say (`static/i18n.js`):
/// `{{ "js-ui js-editor"|js_island|safe }}` gives every message whose
/// id starts with one of those prefixes.
#[askama::filter_fn]
pub fn js_island(prefixes: &str, values: &dyn askama::Values) -> askama::Result<String> {
    let prefixes: Vec<&str> = prefixes.split_whitespace().collect();
    Ok(tr(values)?.script_island(&prefixes))
}

#[askama::filter_fn]
pub fn html_lang(_: &str, values: &dyn askama::Values) -> askama::Result<&'static str> {
    Ok(tr(values)?.html_lang())
}

/// `<html dir="{{ ""|html_dir }}">`.
#[askama::filter_fn]
pub fn html_dir(_: &str, values: &dyn askama::Values) -> askama::Result<&'static str> {
    Ok(tr(values)?.html_dir())
}

/// `<html data-script="{{ ""|html_script }}">`: the script the page's
/// language is written in, which sets its fonts and type
/// (`i18n::scripts`).
#[askama::filter_fn]
pub fn html_script(_: &str, values: &dyn askama::Values) -> askama::Result<&'static str> {
    Ok(tr(values)?.locale().script().code())
}

/// The side a line starts on, for a message's inline styles, which
/// mail clients read without `inline-start`:
/// `padding-{{ ""|start_side }}:9px`.
#[askama::filter_fn]
pub fn start_side(_: &str, values: &dyn askama::Values) -> askama::Result<&'static str> {
    Ok(tr(values)?.locale().dir().start_side())
}

/// A letter-spacing for a mail's inline styles, which reach no
/// stylesheet tokens: as given for a script with case, none for one
/// without (tracking breaks Arabic's joins in Apple Mail's WebKit):
/// `letter-spacing:{{ "-0.2px"|tracking }}`.
#[askama::filter_fn]
pub fn tracking(value: &str, values: &dyn askama::Values) -> askama::Result<String> {
    let cased = tr(values)?.locale().script().style().cased;
    Ok(if cased {
        value.to_string()
    } else {
        "0".to_string()
    })
}

/// Where a link goes for this reader (`Tr::href`):
/// `href="{{ "/guide"|href }}"`.
#[askama::filter_fn]
pub fn href(path: &str, values: &dyn askama::Values) -> askama::Result<String> {
    Ok(tr(values)?.href(path))
}

/// Where a form posts for this reader (`Tr::action`):
/// `action="{{ "/signup"|action }}"`.
#[askama::filter_fn]
pub fn action(path: &str, values: &dyn askama::Values) -> askama::Result<String> {
    Ok(tr(values)?.action(path))
}

/// A public page's links to itself in every language (`Tr::alternates`):
/// `{{ site.base_url|alternates|safe }}`.
#[askama::filter_fn]
pub fn alternates(base: &str, values: &dyn askama::Values) -> askama::Result<String> {
    Ok(tr(values)?.alternates(base))
}

/// The page's `og:locale` tags (`{{ ""|og_locale|safe }}`).
#[askama::filter_fn]
pub fn og_locale(_: &str, values: &dyn askama::Values) -> askama::Result<String> {
    Ok(tr(values)?.og_locale())
}

/// The link offering an English public page in its visitor's
/// language (`{{ ""|offer_link|safe }}`), or nothing.
#[askama::filter_fn]
pub fn offer_link(_: &str, values: &dyn askama::Values) -> askama::Result<String> {
    Ok(tr(values)?.offer_link())
}
