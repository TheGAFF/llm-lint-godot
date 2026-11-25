use super::Document;
use crate::shared::models::Diagnostic;
use std::time::{SystemTime, UNIX_EPOCH};
use toml_edit::DocumentMut;

pub(super) struct Metadata {
    pub body_offset: usize,
    pub body_line: usize,
    pub reviewed: Option<String>,
    pub reviewed_line: usize,
    pub sources_hash: Option<String>,
    pub hash_line: usize,
    pub content: String,
    pub present: bool,
    pub error: Option<String>,
}

impl Metadata {
    pub(super) fn parse(text: &str) -> Self {
        let bom = if text.starts_with('\u{feff}') { 3 } else { 0 };
        let mut result = Self {
            body_offset: bom,
            body_line: 1,
            reviewed: None,
            reviewed_line: 1,
            sources_hash: None,
            hash_line: 1,
            content: String::new(),
            present: false,
            error: None,
        };
        let mut lines = text[bom..].split_inclusive('\n');
        let Some(first) = lines.next() else {
            return result;
        };
        if first.trim_end_matches(['\r', '\n']) != "+++" {
            return result;
        }
        result.present = true;
        let start = bom + first.len();
        let mut end = start;
        for (index, line) in lines.enumerate() {
            if line.trim_end_matches(['\r', '\n']) == "+++" {
                result.body_offset = end + line.len();
                result.body_line = index + 3;
                result.content = text[start..end].into();
                parse_values(&mut result);
                return result;
            }
            end += line.len();
        }
        result.error = Some("Unterminated +++ front matter".into());
        result
    }
}

fn parse_values(metadata: &mut Metadata) {
    match metadata.content.parse::<DocumentMut>() {
        Ok(values) => {
            if let Some((name, _)) = values
                .iter()
                .find(|(name, _)| !["reviewed", "sources_hash"].contains(name))
            {
                metadata.error = Some(format!("Unknown front matter key: {name}"));
            }
            metadata.reviewed = values
                .get("reviewed")
                .and_then(toml_edit::Item::as_str)
                .map(str::to_owned);
            metadata.sources_hash = values
                .get("sources_hash")
                .and_then(toml_edit::Item::as_str)
                .map(str::to_owned);
            metadata.reviewed_line = key_line(&metadata.content, "reviewed");
            metadata.hash_line = key_line(&metadata.content, "sources_hash");
        }
        Err(error) => metadata.error = Some(format!("Invalid TOML front matter: {error}")),
    }
}

fn key_line(content: &str, key: &str) -> usize {
    content
        .lines()
        .position(|line| line.trim_start().starts_with(key))
        .map_or(1, |index| index + 2)
}

impl Document<'_> {
    pub(super) fn validate_metadata(&self, reviewing: bool, diagnostics: &mut Vec<Diagnostic>) {
        let metadata = &self.metadata;
        if let Some(error) = &metadata.error {
            diagnostics.push(self.diagnostic(
                1,
                "metadata",
                error,
                "Use +++ TOML front matter with only reviewed and optional sources_hash string fields",
            ));
            return;
        }
        if !reviewing && !metadata.reviewed.as_deref().is_some_and(valid_date) {
            diagnostics.push(self.diagnostic(
                metadata.reviewed_line,
                "metadata",
                "Missing, non-string, invalid, or future reviewed date",
                "reviewed = \"YYYY-MM-DD\" using a valid non-future UTC date",
            ));
        }
        let check_hash = if reviewing {
            !self.living() && metadata.sources_hash.is_some()
        } else {
            self.living() || metadata.sources_hash.is_some()
        };
        if check_hash && !metadata.sources_hash.as_deref().is_some_and(valid_hash) {
            diagnostics.push(self.diagnostic(
                metadata.hash_line,
                "metadata",
                "Missing or malformed sources_hash",
                "sources_hash = \"sha256:<64 lowercase hexadecimal characters>\"",
            ));
        }
        if metadata.present {
            typed_fields(self, diagnostics);
        }
    }
}

fn typed_fields(document: &Document<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let Ok(values) = document.metadata.content.parse::<DocumentMut>() else {
        return;
    };
    if values.get("reviewed").is_some() && document.metadata.reviewed.is_none() {
        diagnostics.push(document.diagnostic(
            document.metadata.reviewed_line,
            "metadata",
            "reviewed is not a string",
            "Use a quoted YYYY-MM-DD review date",
        ));
    }
    if values.get("sources_hash").is_some() && document.metadata.sources_hash.is_none() {
        diagnostics.push(document.diagnostic(
            document.metadata.hash_line,
            "metadata",
            "sources_hash is not a string",
            "Use a quoted sha256 fingerprint",
        ));
    }
}

pub(super) fn valid_hash(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hash| {
        hash.len() == 64
            && hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

pub(super) fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return false;
    }
    let Some((year, month, day)) = date_parts(value) else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if year == 0 || day == 0 || day > days {
        return false;
    }
    let current = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() / 86400);
    let since_epoch = civil_days(year, month, day);
    since_epoch <= i64::try_from(current).unwrap_or(i64::MAX)
}

fn date_parts(value: &str) -> Option<(i64, i64, i64)> {
    Some((
        value.get(..4)?.parse().ok()?,
        value.get(5..7)?.parse().ok()?,
        value.get(8..)?.parse().ok()?,
    ))
}

fn civil_days(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year / 400;
    let yoe = year - era * 400;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * shifted_month + 2) / 5 + day - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}

pub(super) fn normalized(bytes: &[u8], path: &str) -> Vec<u8> {
    let extension = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if bytes.contains(&0)
        || matches!(
            extension.as_str(),
            "bin"
                | "dat"
                | "res"
                | "scn"
                | "pck"
                | "png"
                | "jpg"
                | "jpeg"
                | "webp"
                | "gif"
                | "ico"
                | "wav"
                | "ogg"
                | "mp3"
                | "flac"
                | "ttf"
                | "otf"
                | "woff"
                | "woff2"
                | "zip"
                | "gz"
                | "br"
                | "7z"
                | "exe"
                | "dll"
                | "so"
                | "aab"
                | "apk"
                | "pdf"
                | "glb"
        )
    {
        return bytes.to_vec();
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return bytes.to_vec();
    };
    let text = text.trim_start_matches('\u{feff}');
    let offset = if extension == "md" {
        Metadata::parse(text).body_offset
    } else {
        0
    };
    text[offset..].replace("\r\n", "\n").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::normalized;

    #[test]
    fn binary_sources_keep_bytes_even_when_their_payload_is_valid_utf8() {
        assert_eq!(normalized(b"RIFF\r\n", "tone.wav"), b"RIFF\r\n");
        assert_eq!(normalized(b"data\r\n", "fixture.bin"), b"data\r\n");
        assert_eq!(normalized(b"zero\0\r\n", "unknown"), b"zero\0\r\n");
        assert_eq!(normalized(b"text\r\n", "source.gd"), b"text\n");
    }
}
