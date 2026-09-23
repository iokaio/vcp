// SPDX-License-Identifier: Apache-2.0
//! Public, immutable prepared file changes. This is never proof of application.
use crate::{workspace::Scope, *};
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "vcp-public-diff/1";
pub const MAX_FILES: usize = 64;
pub const MAX_FILE_BYTES: usize = 1024 * 1024;
pub const MAX_AFTER_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_PATH_BYTES: usize = 32_768;
// At most 64 MiB before + 4 MiB after, base64 encoded, plus worst-case
// escaped paths and bounded identity/field-name overhead.
pub const MAX_DOCUMENT_BYTES: u64 = 112 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Proposed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Content {
    pub sha256: String,
    #[serde(
        rename = "content_base64",
        serialize_with = "serialize_base64",
        deserialize_with = "deserialize_base64"
    )]
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileChange {
    pub path: String,
    pub rename_to: Option<String>,
    pub before: Option<Content>,
    pub after: Option<Content>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub schema: String,
    pub scope: Scope,
    pub change: ToolRunId,
    pub operation_digest: String,
    pub prepared: ArtifactId,
    pub disposition: Disposition,
    #[serde(deserialize_with = "files")]
    pub files: Vec<FileChange>,
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PATH_BYTES
        && !value.chars().any(|c| c.is_control() || c == ':')
        && value
            .split(['/', '\\'])
            .all(|part| !part.is_empty() && part != "." && part != "..")
}
impl FileChange {
    pub fn validate(&self) -> Result<()> {
        if !path(&self.path)
            || self.rename_to.as_ref().is_some_and(|p| !path(p))
            || (self.before.is_none() && self.after.is_none())
            || (self.rename_to.is_some() && (self.before.is_none() || self.after.is_none()))
        {
            return Err(Error::Invalid("public diff path or operation"));
        }
        for content in [&self.before, &self.after].into_iter().flatten() {
            if !hash(&content.sha256) || content.bytes.len() > MAX_FILE_BYTES {
                return Err(Error::Invalid("public diff content"));
            }
        }
        Ok(())
    }
}
impl Document {
    pub fn validate(&self) -> Result<()> {
        if self.schema != SCHEMA
            || !hash(&self.operation_digest)
            || self.files.is_empty()
            || self.files.len() > MAX_FILES
        {
            return Err(Error::Invalid("public diff document"));
        }
        let mut names = std::collections::BTreeSet::new();
        let mut after = 0usize;
        for file in &self.files {
            file.validate()?;
            for name in std::iter::once(&file.path).chain(
                file.rename_to
                    .iter()
                    .filter(|name| !name.eq_ignore_ascii_case(&file.path)),
            ) {
                if !names.insert(name.replace('\\', "/").to_lowercase()) {
                    return Err(Error::Invalid("overlapping public diff paths"));
                }
            }
            after = after
                .checked_add(file.after.as_ref().map_or(0, |v| v.bytes.len()))
                .ok_or(Error::Overflow)?;
        }
        if after > MAX_AFTER_BYTES {
            return Err(Error::Invalid("public diff total bytes"));
        }
        Ok(())
    }
}
fn serialize_base64<S: serde::Serializer>(
    bytes: &[u8],
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(serde::ser::Error::custom("file byte ceiling"));
    }
    use base64::Engine as _;
    serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(bytes))
}
fn deserialize_base64<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> std::result::Result<Vec<u8>, D::Error> {
    struct Encoded;
    impl<'de> serde::de::Visitor<'de> for Encoded {
        type Value = Vec<u8>;
        fn expecting(&self, out: &mut std::fmt::Formatter) -> std::fmt::Result {
            out.write_str("bounded canonical base64 file content")
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> std::result::Result<Vec<u8>, E> {
            base64(value).map_err(E::custom)
        }
    }
    decoder.deserialize_str(Encoded)
}
fn base64(text: &str) -> std::result::Result<Vec<u8>, &'static str> {
    use base64::Engine as _;
    if text.len() % 4 != 0 || text.len() > MAX_FILE_BYTES.div_ceil(3) * 4 {
        return Err("file base64 ceiling");
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text)
        .map_err(|_| "invalid canonical base64")?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err("file byte ceiling");
    }
    Ok(bytes)
}

pub fn deserialize_file_bytes<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> std::result::Result<Vec<u8>, D::Error> {
    struct Bytes;
    impl<'de> serde::de::Visitor<'de> for Bytes {
        type Value = Vec<u8>;
        fn expecting(&self, out: &mut std::fmt::Formatter) -> std::fmt::Result {
            out.write_str("bounded file bytes")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut bytes = Vec::new();
            while let Some(byte) = seq.next_element::<u8>()? {
                if bytes.len() == MAX_FILE_BYTES {
                    return Err(serde::de::Error::custom("file byte ceiling"));
                }
                bytes.push(byte);
            }
            Ok(bytes)
        }
    }
    decoder.deserialize_seq(Bytes)
}
fn files<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> std::result::Result<Vec<FileChange>, D::Error> {
    struct Files;
    impl<'de> serde::de::Visitor<'de> for Files {
        type Value = Vec<FileChange>;
        fn expecting(&self, out: &mut std::fmt::Formatter) -> std::fmt::Result {
            out.write_str("bounded prepared changes")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut files = Vec::new();
            let mut after = 0usize;
            while let Some(file) = seq.next_element::<FileChange>()? {
                if files.len() == MAX_FILES {
                    return Err(serde::de::Error::custom("file count ceiling"));
                }
                after += file.after.as_ref().map_or(0, |v| v.bytes.len());
                if after > MAX_AFTER_BYTES {
                    return Err(serde::de::Error::custom("proposed byte ceiling"));
                }
                file.validate().map_err(serde::de::Error::custom)?;
                files.push(file);
            }
            Ok(files)
        }
    }
    decoder.deserialize_seq(Files)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proposed_changes_reject_paths_unknown_fields_and_oversized_byte_sequences() {
        let change = FileChange {
            path: "src/file.txt".into(),
            rename_to: None,
            before: None,
            after: Some(Content {
                sha256: "a".repeat(64),
                bytes: vec![0, 255],
            }),
        };
        assert!(change.validate().is_ok());
        for path in ["../secret", "C:/secret", "\\secret", "a//b"] {
            let mut invalid = change.clone();
            invalid.path = path.into();
            assert!(invalid.validate().is_err());
        }
        let mut value = serde_json::to_value(&change).unwrap();
        value["native_identity"] = "private".into();
        assert!(serde_json::from_value::<FileChange>(value).is_err());
        let content = Content {
            sha256: "a".repeat(64),
            bytes: vec![0; MAX_FILE_BYTES + 1],
        };
        assert!(serde_json::to_vec(&content).is_err());
        assert!(base64(&"A".repeat(MAX_FILE_BYTES.div_ceil(3) * 4 + 4)).is_err());
        for invalid in ["A", "AA=A", "AB==", "AAB=", "AA==AAAA", "===="] {
            assert!(base64(invalid).is_err());
        }
        for bytes in [
            vec![],
            vec![0],
            vec![0, 255],
            vec![0, 255, 128],
            (0..=255).collect(),
        ] {
            let content = Content {
                sha256: "a".repeat(64),
                bytes,
            };
            assert_eq!(
                serde_json::from_slice::<Content>(&serde_json::to_vec(&content).unwrap()).unwrap(),
                content
            );
        }
    }
}
