/// Words a person would read that a template still writes itself
/// rather than asking the catalog for: text between tags, and the
/// attributes read aloud or shown (`placeholder`, `title`,
/// `aria-label`, `alt`, the confirm prompts). Scripts, styles,
/// drawings, comments and template code are not read. `allowed` are
/// words a page may write as they are (the product's name).
pub fn raw_text(template: &str, allowed: &[&str]) -> Vec<String> {
    let mut text = template.to_string();
    for (open, close) in [
        ("{#", "#}"),
        ("<!--", "-->"),
        ("<script", "</script>"),
        ("<style", "</style>"),
        ("<svg", "</svg>"),
        ("{% block page_class %}", "{% endblock %}"),
    ] {
        text = cut(&text, open, close, " ");
    }
    let mut found = Vec::new();
    let mut rest = text.as_str();
    while let Some(at) = rest.find('<') {
        let Some(end) = rest[at..].find('>') else {
            break;
        };
        let tag = &rest[at..at + end];
        for attr in [
            "placeholder",
            "title",
            "aria-label",
            "alt",
            "hx-confirm",
            "data-confirm",
            "data-busy-label",
            "data-placeholder",
        ] {
            for quote in ['"', '\''] {
                let key = format!(" {attr}={quote}");
                if let Some(v) = tag.find(&key) {
                    let value = &tag[v + key.len()..];
                    let value = &value[..value.find(quote).unwrap_or(value.len())];
                    let value = cut(&cut(value, "{{", "}}", " "), "{%", "%}", " ");
                    if words(&value, allowed) {
                        found.push(format!("{attr}=\"{}\"", value.trim()));
                    }
                }
            }
        }
        rest = &rest[at + end..];
    }
    let text = cut(
        &cut(&cut(&text, "{{", "}}", " "), "{%", "%}", " "),
        "<",
        ">",
        "\n",
    );
    for piece in text.split('\n') {
        let piece = piece.trim();
        if words(piece, allowed) {
            found.push(piece.to_string());
        }
    }
    found
}

/// `text` with every `open`...`close` span replaced by `with`.
pub(super) fn cut(text: &str, open: &str, close: &str, with: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(a) = rest.find(open) {
        out.push_str(&rest[..a]);
        match rest[a + open.len()..].find(close) {
            Some(b) => {
                out.push_str(with);
                rest = &rest[a + open.len() + b + close.len()..];
            }
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// The tags that end a line of what a reader sees: a paragraph, a
/// heading, a list item, a cell, a control.
const BLOCKS: &[&str] = &[
    "p",
    "li",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "div",
    "section",
    "article",
    "header",
    "footer",
    "nav",
    "main",
    "aside",
    "blockquote",
    "pre",
    "ul",
    "ol",
    "dl",
    "dt",
    "dd",
    "table",
    "tr",
    "td",
    "th",
    "caption",
    "figcaption",
    "details",
    "summary",
    "br",
    "hr",
    "form",
    "label",
    "button",
    "option",
];

/// The entities a page writes, read as the characters they stand for.
const READ_ENTITIES: &[(&str, &str)] = &[
    ("amp", "&"),
    ("lt", "<"),
    ("gt", ">"),
    ("quot", "\""),
    ("apos", "'"),
    ("nbsp", "\u{A0}"),
    ("lsquo", "\u{2018}"),
    ("rsquo", "\u{2019}"),
    ("ldquo", "\u{201C}"),
    ("rdquo", "\u{201D}"),
    ("laquo", "\u{AB}"),
    ("raquo", "\u{BB}"),
    ("lsaquo", "\u{2039}"),
    ("rsaquo", "\u{203A}"),
    ("larr", "\u{2190}"),
    ("rarr", "\u{2192}"),
    ("hellip", "\u{2026}"),
    ("middot", "\u{B7}"),
    ("times", "\u{D7}"),
    ("ndash", "\u{2013}"),
    ("iexcl", "\u{A1}"),
    ("iquest", "\u{BF}"),
];

/// What a reader gets from `html`, one paragraph, heading, list item or
/// cell to a line: tags, template code, comments, scripts and styles
/// dropped; the attributes a reader is shown or read (`alt`, `title`,
/// `aria-label`) each a line of their own; entities read as their
/// characters. An entity it does not know stays as written, so one that
/// spells a dash still reads as a dash to `unreadable`.
pub fn shown_html(html: &str) -> String {
    let mut text = html.to_string();
    for (open, close) in [
        ("{#", "#}"),
        ("<!--", "-->"),
        ("<script", "</script>"),
        ("<style", "</style>"),
        ("<svg", "</svg>"),
        ("{{", "}}"),
        ("{%", "%}"),
    ] {
        text = cut(&text, open, close, " ");
    }
    // HTML folds only ASCII white space: a no-break space stays one.
    let text = text.split_ascii_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = String::new();
    let mut attributes = Vec::new();
    let mut rest = text.as_str();
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        let Some(end) = rest[at..].find('>') else {
            break;
        };
        let tag = &rest[at + 1..at + end];
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_lowercase();
        if BLOCKS.contains(&name.as_str()) {
            out.push('\n');
        }
        for attr in ["alt", "title", "aria-label"] {
            for quote in ['"', '\''] {
                let key = format!(" {attr}={quote}");
                if let Some(v) = tag.find(&key) {
                    let value = &tag[v + key.len()..];
                    attributes.push(value[..value.find(quote).unwrap_or(value.len())].to_string());
                }
            }
        }
        rest = &rest[at + end + 1..];
    }
    out.push_str(rest);
    for value in attributes {
        out.push('\n');
        out.push_str(&value);
    }
    read_entities(&out)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// `text` with each entity a page writes (`READ_ENTITIES`, `&#8217;`,
/// `&#x2019;`) read as its character, and any other kept as written.
fn read_entities(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 1..];
        let name: String = tail
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '#')
            .collect();
        let read = if tail[name.len()..].starts_with(';') {
            match name.strip_prefix('#') {
                Some(n) => match n.strip_prefix('x') {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => n.parse().ok(),
                }
                .and_then(char::from_u32)
                .map(String::from),
                None => READ_ENTITIES
                    .iter()
                    .find(|(e, _)| *e == name)
                    .map(|(_, c)| (*c).to_string()),
            }
        } else {
            None
        };
        match read {
            Some(c) => {
                out.push_str(&c);
                rest = &tail[name.len() + 1..];
            }
            None => {
                out.push('&');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Whether `text` holds a word (two letters in a row) that is not one
/// of `allowed`.
fn words(text: &str, allowed: &[&str]) -> bool {
    let mut plain = String::new();
    let mut rest = text;
    while let Some(a) = rest.find('&') {
        plain.push_str(&rest[..a]);
        let tail = &rest[a + 1..];
        match tail.find(';').filter(|&e| {
            e <= 8
                && tail[..e]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '#')
        }) {
            Some(e) => rest = &tail[e + 1..],
            None => {
                plain.push('&');
                rest = tail;
            }
        }
    }
    plain.push_str(rest);
    plain
        .split(|c: char| !c.is_alphanumeric() && c != '@' && c != '.')
        .map(|w| w.trim_matches('.'))
        .filter(|w| !w.is_empty() && !allowed.contains(w))
        .any(|w| {
            let letters: Vec<bool> = w.chars().map(char::is_alphabetic).collect();
            letters.windows(2).any(|p| p[0] && p[1])
        })
}
