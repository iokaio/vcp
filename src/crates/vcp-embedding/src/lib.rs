// SPDX-License-Identifier: Apache-2.0
//! File-only, CPU MiniLM qualification adapter. No acquisition or remote fallback.
//! Candle API reference: huggingface/candle BERT example at
//! ddf1b879dc3a1760cbcb3f3c4a7c6467850cec4a (MIT OR Apache-2.0).
//! Published Candle 0.11.0 source revision: 31f35b147389700ed2a178ee66a91c3cc25cc80d.
use candle_core::{Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config, DTYPE};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt, fs::File, io::Read, path::Path};
use tokenizers::{PaddingParams, Tokenizer, TruncationParams};

pub const DIMENSIONS: usize = 384;
pub const MAX_TOKENS: usize = 256;
pub const MAX_BATCH: usize = 16;
pub const MAX_INPUT_BYTES: usize = 64 * 1024;
pub const ASSET_SPEC: &str = include_str!("../../../third_party/components/minilm-assets.json");

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Error {
    MissingAsset { file: String },
    InvalidAsset { file: String, reason: String },
    InvalidInput { reason: String },
    Runtime { reason: String },
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            serde_json::to_string(self).map_err(|_| fmt::Error)?
        )
    }
}
impl std::error::Error for Error {}
fn invalid(file: &str, reason: &str) -> Error {
    Error::InvalidAsset {
        file: file.into(),
        reason: reason.into(),
    }
}
fn runtime(error: impl fmt::Display) -> Error {
    Error::Runtime {
        reason: error.to_string(),
    }
}

#[derive(Deserialize)]
struct Asset {
    path: String,
    bytes: u64,
    sha256: String,
}
#[derive(Deserialize)]
struct Specification {
    files: Vec<Asset>,
}

fn verified_bytes(root: &Path, asset: &Asset) -> Result<Vec<u8>, Error> {
    let file = File::open(root.join(&asset.path)).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            Error::MissingAsset {
                file: asset.path.clone(),
            }
        } else {
            invalid(&asset.path, "cannot open local asset")
        }
    })?;
    // A bounded read covers growth/replacement races without unbounded allocation.
    let mut bytes = Vec::new();
    file.take(asset.bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid(&asset.path, "cannot read local asset"))?;
    if bytes.len() as u64 != asset.bytes {
        return Err(invalid(&asset.path, "size mismatch"));
    }
    if format!("{:x}", Sha256::digest(&bytes)) != asset.sha256 {
        return Err(invalid(&asset.path, "SHA-256 mismatch"));
    }
    Ok(bytes)
}

#[derive(Debug, Serialize)]
pub struct Embedding {
    pub vector: Vec<f32>,
    /// Active wordpieces including special tokens, excluding batch padding.
    pub tokens_used: usize,
    pub truncated: bool,
}

pub struct MiniLm {
    model: BertModel,
    tokenizer: Tokenizer,
}
impl MiniLm {
    /// Load only the compiled, immutable asset selection from an explicitly supplied root.
    /// Missing/corrupt assets return a visible setup error; this never downloads files.
    pub fn load(root: &Path) -> Result<Self, Error> {
        let spec: Specification = serde_json::from_str(ASSET_SPEC).map_err(runtime)?;
        let mut inputs = BTreeMap::new();
        for asset in spec.files {
            let bytes = verified_bytes(root, &asset)?;
            // Hash every selected asset, retain only model inputs; consume these exact bytes.
            if ["config.json", "tokenizer.json", "model.safetensors"].contains(&asset.path.as_str())
            {
                inputs.insert(asset.path, bytes);
            }
        }
        let take = |inputs: &mut BTreeMap<String, Vec<u8>>, name: &str| {
            inputs
                .remove(name)
                .ok_or_else(|| invalid(name, "missing compiled specification entry"))
        };
        let config: Config = serde_json::from_slice(&take(&mut inputs, "config.json")?)
            .map_err(|_| invalid("config.json", "invalid BERT configuration"))?;
        if config.hidden_size != DIMENSIONS
            || config.num_hidden_layers != 6
            || config.max_position_embeddings < MAX_TOKENS
        {
            return Err(invalid("config.json", "unsupported model architecture"));
        }
        let weights = take(&mut inputs, "model.safetensors")?;
        let variables = VarBuilder::from_buffered_safetensors(weights, DTYPE, &Device::Cpu)
            .map_err(|_| invalid("model.safetensors", "invalid tensor archive"))?;
        let model = BertModel::load(variables, &config)
            .map_err(|_| invalid("model.safetensors", "incompatible model tensors"))?;
        let mut tokenizer = Tokenizer::from_bytes(take(&mut inputs, "tokenizer.json")?)
            .map_err(|_| invalid("tokenizer.json", "invalid tokenizer"))?;
        tokenizer.with_padding(Some(PaddingParams::default()));
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: MAX_TOKENS,
                ..Default::default()
            }))
            .map_err(|_| invalid("tokenizer.json", "incompatible truncation configuration"))?;
        Ok(Self { model, tokenizer })
    }

    pub fn specification_sha256() -> String {
        format!("{:x}", Sha256::digest(ASSET_SPEC.as_bytes()))
    }

    pub fn embed(&self, texts: &[&str]) -> Result<Vec<Embedding>, Error> {
        validate_inputs(texts)?;
        let tokens = self
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(runtime)?;
        let matrix = |select: fn(&tokenizers::Encoding) -> &[u32]| -> Result<Tensor, Error> {
            let rows = tokens
                .iter()
                .map(|t| Tensor::new(select(t), &Device::Cpu))
                .collect::<candle_core::Result<Vec<_>>>()
                .map_err(runtime)?;
            Tensor::stack(&rows, 0).map_err(runtime)
        };
        let ids = matrix(tokenizers::Encoding::get_ids)?;
        let types = matrix(tokenizers::Encoding::get_type_ids)?;
        let mask = matrix(tokenizers::Encoding::get_attention_mask)?;
        let hidden = self
            .model
            .forward(&ids, &types, Some(&mask))
            .map_err(runtime)?
            .to_vec3::<f32>()
            .map_err(runtime)?;
        hidden
            .iter()
            .zip(&tokens)
            .map(|(row, encoding)| {
                let vector = pool(row, encoding.get_attention_mask())?;
                Ok(Embedding {
                    vector,
                    tokens_used: encoding
                        .get_attention_mask()
                        .iter()
                        .filter(|v| **v != 0)
                        .count(),
                    truncated: !encoding.get_overflowing().is_empty(),
                })
            })
            .collect()
    }
}

fn validate_inputs(texts: &[&str]) -> Result<(), Error> {
    if texts.is_empty()
        || texts.len() > MAX_BATCH
        || texts
            .iter()
            .any(|s| s.trim().is_empty() || s.len() > MAX_INPUT_BYTES)
    {
        return Err(Error::InvalidInput {
            reason: "expected 1..16 nonempty texts, each at most 65536 UTF-8 bytes".into(),
        });
    }
    Ok(())
}
fn pool(tokens: &[Vec<f32>], mask: &[u32]) -> Result<Vec<f32>, Error> {
    if tokens.len() != mask.len() {
        return Err(runtime("token/mask shape mismatch"));
    }
    let mut vector = vec![0f32; DIMENSIONS];
    let mut count = 0f32;
    for (token, valid) in tokens.iter().zip(mask) {
        if *valid == 0 {
            continue;
        }
        if token.len() != DIMENSIONS {
            return Err(runtime("embedding dimension mismatch"));
        }
        for (out, value) in vector.iter_mut().zip(token) {
            *out += value;
        }
        count += 1.;
    }
    if count == 0. {
        return Err(runtime("no active embedding tokens"));
    }
    for value in &mut vector {
        *value /= count;
    }
    let norm = vector
        .iter()
        .map(|v| (*v as f64).powi(2))
        .sum::<f64>()
        .sqrt();
    if !norm.is_finite() || norm == 0. {
        return Err(runtime("nonfinite or zero embedding norm"));
    }
    for value in &mut vector {
        *value = (*value as f64 / norm) as f32;
    }
    Ok(vector)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_and_corrupt_assets_are_explicit_setup_errors() {
        let root = tempfile::tempdir().unwrap();
        assert!(matches!(
            MiniLm::load(root.path()),
            Err(Error::MissingAsset { .. })
        ));
        let asset = Asset {
            path: "synthetic".into(),
            bytes: 3,
            sha256: format!("{:x}", Sha256::digest(b"abc")),
        };
        std::fs::write(root.path().join("synthetic"), b"abd").unwrap();
        assert!(
            matches!(verified_bytes(root.path(), &asset), Err(Error::InvalidAsset { reason, .. }) if reason == "SHA-256 mismatch")
        );
        std::fs::write(root.path().join("synthetic"), b"abc-extra").unwrap();
        assert!(
            matches!(verified_bytes(root.path(), &asset), Err(Error::InvalidAsset { reason, .. }) if reason == "size mismatch")
        );
    }
    #[test]
    fn invalid_requests_fail_before_tokenization() {
        assert!(validate_inputs(&[]).is_err());
        assert!(validate_inputs(&["  "]).is_err());
        assert!(validate_inputs(&["x"; MAX_BATCH + 1]).is_err());
        assert!(validate_inputs(&[&"x".repeat(MAX_INPUT_BYTES + 1)]).is_err());
        assert!(validate_inputs(&["Unicode café 東京"]).is_ok());
    }
    #[test]
    fn invalid_vectors_never_enter_an_index() {
        assert!(pool(&[vec![0.; DIMENSIONS]], &[1]).is_err());
        assert!(pool(&[vec![f32::NAN; DIMENSIONS]], &[1]).is_err());
        assert!(pool(&[vec![1.; DIMENSIONS]], &[0]).is_err());
        assert!(pool(&[vec![1.; DIMENSIONS - 1]], &[1]).is_err());
        assert!(pool(&[vec![1.; DIMENSIONS]], &[]).is_err());
    }
}
