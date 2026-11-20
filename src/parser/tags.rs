//! Splits simfile text into `#TAG:value;` pairs.

/// Removes the UTF-8 BOM and `//` comments.
fn strip_comments(text: &str) -> String {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        match line.find("//") {
            Some(pos) => {
                out.push_str(&line[..pos]);
                if line.ends_with('\n') {
                    out.push('\n');
                }
            }
            None => out.push_str(line),
        }
    }
    out
}

/// Returns `(TAG, value)` pairs in file order. Tag names are upper-cased.
///
/// A value normally ends at `;`. Like StepMania, a missing `;` is tolerated:
/// the value also ends when a new line starts with `#`.
pub(crate) fn split_tags(text: &str) -> Vec<(String, String)> {
    let text = strip_comments(text);
    let bytes = text.as_bytes();
    let mut tags = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] != b'#' {
            i += 1;
            continue;
        }
        let Some(colon) = text[i..].find(':').map(|p| p + i) else {
            break;
        };
        let name = text[i + 1..colon].trim().to_ascii_uppercase();

        let start = colon + 1;
        let mut end = start;
        let mut at_line_start = false;
        while end < bytes.len() {
            match bytes[end] {
                b';' => break,
                b'\n' => at_line_start = true,
                b'#' if at_line_start => break,
                b' ' | b'\t' | b'\r' => {}
                _ => at_line_start = false,
            }
            end += 1;
        }
        tags.push((name, text[start..end].to_string()));
        // Skip the `;`, but not a `#` that already starts the next tag.
        i = if bytes.get(end) == Some(&b';') {
            end + 1
        } else {
            end
        };
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_basic_tags() {
        let tags = split_tags("#TITLE:Song;\n#ARTIST:Me;");
        assert_eq!(tags[0], ("TITLE".into(), "Song".into()));
        assert_eq!(tags[1], ("ARTIST".into(), "Me".into()));
    }

    #[test]
    fn handles_bom_comments_crlf_and_missing_semicolon() {
        let text = "\u{feff}#TITLE:A // comment\r\n#ARTIST:B\r\n#BPMS:0=120;";
        let tags = split_tags(text);
        assert_eq!(tags.len(), 3);
        assert_eq!(tags[0].1.trim(), "A");
        assert_eq!(tags[1].1.trim(), "B");
        assert_eq!(tags[2].1, "0=120");
    }

    #[test]
    fn values_can_contain_colons() {
        let tags = split_tags("#NOTES:dance-single:desc:Hard:9::0000;");
        assert_eq!(tags[0].1, "dance-single:desc:Hard:9::0000");
    }
}
