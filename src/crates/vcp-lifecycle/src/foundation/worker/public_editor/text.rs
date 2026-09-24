// SPDX-License-Identifier: Apache-2.0
//! Exact UTF-16 editor positions over logical UTF-8 document content.
use super::*;

fn offset(text: &str, line: u32, character: u32) -> Result<usize> {
    let mut start = 0;
    for _ in 0..line {
        start += text[start..]
            .find(['\r', '\n'])
            .ok_or("editor line is outside document")?;
        start += if text[start..].starts_with("\r\n") {
            2
        } else {
            1
        };
    }
    let rest = &text[start..];
    let end = rest.find(['\r', '\n']).unwrap_or(rest.len());
    let line = &rest[..end];
    let mut utf16 = 0usize;
    for (byte, value) in line.char_indices() {
        if utf16 == character as usize {
            return Ok(start + byte);
        }
        utf16 += value.len_utf16();
        if utf16 > character as usize {
            return Err("editor position splits a surrogate pair".into());
        }
    }
    if utf16 == character as usize {
        Ok(start + line.len())
    } else {
        Err("editor character is outside line".into())
    }
}

pub(super) fn replace(
    text: &str,
    edits: &[wire::TextEdit],
    eol: &wire::EndOfLine,
) -> Result<String> {
    let mut spans = edits
        .iter()
        .map(|edit| {
            let start = offset(text, edit.range.start.line, edit.range.start.character)?;
            let end = offset(text, edit.range.end.line, edit.range.end.character)?;
            if start > end {
                return Err("editor range is reversed".into());
            }
            let text = edit.text.replace("\r\n", "\n").replace('\r', "\n");
            let text = if matches!(eol, wire::EndOfLine::Crlf) {
                text.replace('\n', "\r\n")
            } else {
                text
            };
            Ok((start, end, text))
        })
        .collect::<Result<Vec<_>>>()?;
    spans.sort_by_key(|(start, end, _)| (*start, *end));
    for pair in spans.windows(2) {
        if pair[0].1 > pair[1].0 || pair[0].0 == pair[1].0 {
            return Err("editor edits overlap or share an insertion position".into());
        }
    }
    let length = spans
        .iter()
        .try_fold(text.len(), |length, (start, end, replacement)| {
            length
                .checked_sub(end - start)
                .and_then(|length| length.checked_add(replacement.len()))
                .ok_or("editor replacement length overflow")
        })?;
    if length > 65536 {
        return Err("editor replacement exceeds content bound".into());
    }
    let mut result = text.to_owned();
    for (start, end, replacement) in spans.into_iter().rev() {
        result.replace_range(start..end, &replacement);
    }
    if result == text {
        return Err("editor proposal does not change content".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn edit(start: u32, end: u32, replacement: &str) -> wire::TextEdit {
        wire::TextEdit {
            range: wire::Range {
                start: wire::Position {
                    line: 0,
                    character: start,
                },
                end: wire::Position {
                    line: 0,
                    character: end,
                },
            },
            text: replacement.into(),
        }
    }
    #[test]
    fn utf16_offsets_preserve_crlf_and_reject_surrogate_splits() {
        let text = "a😀b\r\nnext\n";
        assert_eq!(offset(text, 0, 3).unwrap(), 5);
        assert_eq!(offset(text, 0, 4).unwrap(), 6);
        assert_eq!(offset(text, 1, 0).unwrap(), 8);
        assert_eq!(offset(text, 2, 0).unwrap(), text.len());
        assert!(offset(text, 0, 2).is_err());
        assert!(offset(text, 0, 5).is_err());
        assert!(offset(text, 3, 0).is_err());
        assert_eq!(offset("a\rb", 1, 0).unwrap(), 2);
        assert_eq!(offset("a\rb", 0, 1).unwrap(), 1);
    }
    #[test]
    fn edits_use_original_positions_and_reject_ambiguous_insertions() {
        let replace = |text: &str, edits: &[wire::TextEdit]| {
            super::replace(text, edits, &wire::EndOfLine::Crlf)
        };
        assert_eq!(
            replace("a😀bc\r\n", &[edit(4, 5, "last"), edit(1, 3, "X")]).unwrap(),
            "aXblast\r\n"
        );
        assert!(replace("abcd", &[edit(1, 3, "x"), edit(2, 4, "y")]).is_err());
        assert!(replace("abcd", &[edit(1, 1, "x"), edit(1, 1, "y")]).is_err());
        assert!(replace("abcd", &[edit(3, 1, "x")]).is_err());
        assert!(replace("😀", &[edit(1, 2, "x")]).is_err());
        assert_eq!(
            replace("abcd", &[edit(0, 1, "x"), edit(1, 2, "y")]).unwrap(),
            "xycd"
        );
    }
}
