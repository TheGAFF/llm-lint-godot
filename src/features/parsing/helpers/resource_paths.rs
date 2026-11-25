pub(crate) fn strings(text: &str) -> impl Iterator<Item = &str> {
    let mut chars = text.char_indices();
    std::iter::from_fn(move || {
        let (start, quote) = chars.find(|&(_, ch)| matches!(ch, '"' | '\''))?;
        while let Some((end, ch)) = chars.next() {
            if ch == quote {
                return Some(&text[start + 1..end]);
            }
            if ch == '\\' {
                chars.next()?;
            }
        }
        None
    })
}

pub(crate) fn attribute<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.match_indices(key).find_map(|(start, _)| {
        if start == 0 || !line[..start].ends_with(char::is_whitespace) {
            return None;
        }
        let value = line[start + key.len()..]
            .trim_start()
            .strip_prefix('=')?
            .trim_start();
        if !value.starts_with('"') {
            return None;
        }
        strings(value).next()
    })
}

pub(crate) fn normalized(base: &str, path: &str) -> Option<String> {
    if path.contains([':', '\\']) || path.starts_with('/') {
        return None;
    }
    let mut parts = Vec::new();
    for part in base.split('/').chain(path.split('/')) {
        match part {
            "" | "." => (),
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

pub(crate) fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

pub(crate) fn resolve(
    uids: &std::collections::BTreeMap<String, String>,
    base: &str,
    raw: &str,
) -> Option<String> {
    let path = raw.split("::").next()?;
    if let Some(relative) = path.strip_prefix("res://") {
        normalized("", relative)
    } else if path.starts_with("uid://") {
        uids.get(path).cloned()
    } else {
        normalized(base, path)
    }
}
