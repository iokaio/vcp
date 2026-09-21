// SPDX-License-Identifier: Apache-2.0
//! Explicit presentation decoding; raw capture and exit semantics stay authoritative.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Utf8,
    Utf16Le,
}
#[derive(Clone, Debug, Serialize)]
pub struct Decoded {
    pub encoding: Encoding,
    pub decision: &'static str,
    pub tail: String,
    pub replacement_characters: usize,
    pub omitted_bytes: u64,
    pub split_prefix_bytes: usize,
}
pub fn decode(bytes: &[u8], total: u64, declared: Option<Encoding>) -> Decoded {
    let encoding = declared.unwrap_or(Encoding::Utf8);
    let omitted = total.saturating_sub(bytes.len() as u64);
    let mut result = Decoded {
        encoding,
        decision: if declared.is_some() {
            "trusted_profile"
        } else {
            "legacy_utf8"
        },
        tail: String::new(),
        replacement_characters: 0,
        omitted_bytes: omitted,
        split_prefix_bytes: 0,
    };
    match encoding {
        Encoding::Utf8 => {
            let skip = if omitted > 0 {
                bytes
                    .iter()
                    .take(3)
                    .take_while(|b| **b & 0xc0 == 0x80)
                    .count()
            } else {
                0
            };
            result.split_prefix_bytes = skip;
            let mut input = &bytes[skip..];
            while !input.is_empty() {
                match std::str::from_utf8(input) {
                    Ok(text) => {
                        result.tail.push_str(text);
                        break;
                    }
                    Err(error) => {
                        let valid = error.valid_up_to();
                        // The decoder has proved this prefix valid.
                        result
                            .tail
                            .push_str(std::str::from_utf8(&input[..valid]).unwrap_or_default());
                        result.tail.push('\u{fffd}');
                        result.replacement_characters += 1;
                        input = &input[valid + error.error_len().unwrap_or(input.len() - valid)..];
                    }
                }
            }
        }
        Encoding::Utf16Le => {
            let mut skip = if omitted % 2 == 1 {
                1.min(bytes.len())
            } else {
                0
            };
            if omitted > 0 && bytes.len() >= skip + 2 {
                let first = u16::from_le_bytes([bytes[skip], bytes[skip + 1]]);
                if (0xdc00..=0xdfff).contains(&first) {
                    skip += 2;
                }
            }
            result.split_prefix_bytes = skip;
            let input = &bytes[skip..];
            for decoded in char::decode_utf16(
                input
                    .chunks_exact(2)
                    .map(|b| u16::from_le_bytes([b[0], b[1]])),
            ) {
                result.tail.push(decoded.unwrap_or_else(|_| {
                    result.replacement_characters += 1;
                    '\u{fffd}'
                }));
            }
            if input.len() % 2 != 0 {
                result.tail.push('\u{fffd}');
                result.replacement_characters += 1;
            }
        }
    }
    result.omitted_bytes += result.split_prefix_bytes as u64;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_decoding_labels_loss_without_changing_raw_input() {
        let raw = b"\x1b[31m\xc3\xa9\xff\xef\xbf\xbd";
        let result = decode(raw, raw.len() as u64, None);
        assert_eq!(result.tail, "\x1b[31mé\u{fffd}\u{fffd}");
        assert_eq!(result.replacement_characters, 1);
        assert_eq!(result.decision, "legacy_utf8");
        assert_eq!(raw[7], 0xff);
        let split = decode(b"\x98\x80ok", 6, Some(Encoding::Utf8));
        assert_eq!(split.tail, "ok");
        assert_eq!(split.omitted_bytes, 4);
        assert_eq!(split.replacement_characters, 0);
        assert_eq!(decode(b"\xe2\x82", 2, None).replacement_characters, 1);
    }
    #[test]
    fn utf16_aligns_split_units_and_surrogates_and_discloses_invalid_bytes() {
        let raw: Vec<u8> = "é😀ok".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(
            decode(&raw, raw.len() as u64, Some(Encoding::Utf16Le)).tail,
            "é😀ok"
        );
        for offset in [3, 4, 5] {
            let result = decode(&raw[offset..], raw.len() as u64, Some(Encoding::Utf16Le));
            assert_eq!(result.tail, "ok");
            assert_eq!(result.replacement_characters, 0);
            assert_eq!(result.omitted_bytes, 6);
        }
        assert_eq!(
            decode(&[0, 0xd8, 1], 3, Some(Encoding::Utf16Le)).replacement_characters,
            2
        );
    }
}
