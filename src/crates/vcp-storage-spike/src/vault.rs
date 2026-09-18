// SPDX-License-Identifier: Apache-2.0
//! Sign the complete inner inventory, then encrypt every published object with
//! age. Recipient possession never enrolls a writer or overrides local lineage.
use crate::{digest, object_id, reject, Result, View, LIMIT};
use age::x25519::{Identity, Recipient};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
const DOMAIN: &[u8] = b"vcp-p0-snapshot-signature-v1\0";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Object {
    pub ciphertext: String,
    pub bytes: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: u32,
    view: View,
    parent: Option<String>,
    objects: BTreeMap<String, Object>,
    inline: BTreeMap<String, Vec<u8>>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Signed {
    writer: [u8; 32],
    body: Vec<u8>,
    signature: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct Published {
    pub id: String,
    pub objects: BTreeMap<String, Object>,
    pub recipient: String,
    pub transferred: u64,
}
pub struct Trust {
    pub writers: BTreeSet<[u8; 32]>,
    pub minimum_sequence: u64,
    pub minimum_deletion: u64,
    pub parent: Option<String>,
}
#[derive(Clone, Copy, PartialEq)]
pub enum Fault {
    None,
    BeforePublication,
    AfterObjects,
}
pub struct Restored {
    pub view: View,
    pub artifacts: BTreeMap<String, Vec<u8>>,
}
// Private construction: publication accepts finalized ciphertext, never a path
// supplied by a caller claiming that its contents are encrypted.
struct Ciphertext(Vec<u8>);
impl Ciphertext {
    fn encrypt(recipient: &Recipient, bytes: &[u8]) -> Result<Self> {
        if bytes.len() > LIMIT {
            return Err(reject("plaintext limit"));
        }
        let value = age::encrypt(recipient, bytes)?;
        if value.len() > LIMIT + 4096 {
            return Err(reject("ciphertext limit"));
        }
        Ok(Self(value))
    }
    fn publish(&self, stage: &Path, vault: &Path) -> Result<String> {
        let id = digest(&self.0);
        let temp = stage.join(format!("{id}.cipher"));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        file.write_all(&self.0)?;
        file.sync_all()?;
        drop(file);
        let destination = vault.join(format!("{id}.age"));
        if destination.exists() {
            if read(&destination, LIMIT + 4096)? != self.0 {
                return Err(reject("existing ciphertext corrupt"));
            }
        } else {
            let partial = vault.join(format!("{id}.partial"));
            let mut output = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&partial)?;
            output.write_all(&self.0)?;
            output.sync_all()?;
            drop(output);
            fs::rename(&partial, &destination)?;
        }
        fs::remove_file(temp)?;
        Ok(id)
    }
}
fn directories(stage: &Path, vault: &Path) -> Result<(PathBuf, PathBuf)> {
    let stage = fs::canonicalize(stage)?;
    let vault = fs::canonicalize(vault)?;
    if stage.starts_with(&vault) || vault.starts_with(&stage) {
        return Err(reject("staging and vault must be disjoint"));
    }
    Ok((stage, vault))
}
pub fn publish(
    view: &View,
    artifacts: &BTreeMap<String, Vec<u8>>,
    recipient: &Recipient,
    writer: &SigningKey,
    stage: &Path,
    vault: &Path,
    previous: Option<&Published>,
    incremental: bool,
    fault: Fault,
) -> Result<Published> {
    view.validate()?;
    validate_artifacts(view, artifacts)?;
    let (stage, vault) = directories(stage, vault)?;
    if fault == Fault::BeforePublication {
        return Err(reject("injected encryption/publication barrier"));
    }
    let mut manifest = Manifest {
        format: 1,
        view: view.clone(),
        parent: previous.map(|p| p.id.clone()),
        objects: BTreeMap::new(),
        inline: BTreeMap::new(),
    };
    let mut transferred = 0;
    for (id, bytes) in artifacts {
        if !incremental {
            manifest.inline.insert(id.clone(), bytes.clone());
            continue;
        }
        let reusable = previous
            .filter(|p| p.recipient == recipient.to_string())
            .and_then(|p| p.objects.get(id));
        let object = if let Some(old) = reusable {
            let cipher = read_object(&vault, &old.ciphertext)?;
            if cipher.len() as u64 != old.bytes {
                return Err(reject("cached ciphertext size"));
            }
            old.clone()
        } else {
            let cipher = Ciphertext::encrypt(recipient, bytes)?;
            let bytes = cipher.0.len() as u64;
            let ciphertext = cipher.publish(&stage, &vault)?;
            transferred += bytes;
            Object { ciphertext, bytes }
        };
        manifest.objects.insert(id.clone(), object);
    }
    if fault == Fault::AfterObjects {
        return Err(reject("injected incomplete upload"));
    }
    let body = serde_json::to_vec(&manifest)?;
    let signature = writer
        .sign(&[DOMAIN, body.as_slice()].concat())
        .to_bytes()
        .to_vec();
    let signed = serde_json::to_vec(&Signed {
        writer: writer.verifying_key().to_bytes(),
        body,
        signature,
    })?;
    let cipher = Ciphertext::encrypt(recipient, &signed)?;
    transferred += cipher.0.len() as u64;
    let id = cipher.publish(&stage, &vault)?;
    Ok(Published {
        id,
        objects: manifest.objects,
        recipient: recipient.to_string(),
        transferred,
    })
}
pub fn restore(vault: &Path, id: &str, identity: &Identity, trust: &Trust) -> Result<Restored> {
    let signed = decrypt(identity, &read_object(vault, id)?)?;
    let signed: Signed = serde_json::from_slice(&signed)?;
    if !trust.writers.contains(&signed.writer) {
        return Err(reject("unenrolled writer"));
    }
    let key = VerifyingKey::from_bytes(&signed.writer)?;
    key.verify_strict(
        &[DOMAIN, signed.body.as_slice()].concat(),
        &Signature::from_slice(&signed.signature)?,
    )?;
    let manifest: Manifest = serde_json::from_slice(&signed.body)?;
    manifest.view.validate()?;
    if manifest.format != 1
        || manifest.view.sequence <= trust.minimum_sequence
        || manifest.view.deletion_epoch < trust.minimum_deletion
        || manifest.parent != trust.parent
        || (!manifest.objects.is_empty() && !manifest.inline.is_empty())
    {
        return Err(reject("incompatible, replayed or divergent snapshot"));
    }
    let mut artifacts = manifest.inline;
    for (id, object) in manifest.objects {
        let encrypted = read_object(vault, &object.ciphertext)?;
        if encrypted.len() as u64 != object.bytes {
            return Err(reject("ciphertext size mismatch"));
        }
        artifacts.insert(id, decrypt(identity, &encrypted)?);
    }
    validate_artifacts(&manifest.view, &artifacts)?;
    Ok(Restored {
        view: manifest.view,
        artifacts,
    })
}
fn validate_artifacts(view: &View, artifacts: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    if view.artifacts.len() != artifacts.len() {
        return Err(reject("artifact closure mismatch"));
    }
    let mut total = 0usize;
    for (id, bytes) in artifacts {
        total = total
            .checked_add(bytes.len())
            .ok_or_else(|| reject("artifact overflow"))?;
        if total > LIMIT
            || view.artifacts.get(id) != Some(&(bytes.len() as u64))
            || digest(bytes) != *id
        {
            return Err(reject("artifact inventory mismatch"));
        }
    }
    Ok(())
}
pub fn decrypt(identity: &Identity, bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.len() > LIMIT + 4096 {
        return Err(reject("encrypted input too large"));
    }
    let decryptor = age::Decryptor::new(bytes)?;
    let mut reader = decryptor
        .decrypt(std::iter::once(identity as &dyn age::Identity))?
        .take((LIMIT + 1) as u64);
    let mut plaintext = Vec::new();
    reader.read_to_end(&mut plaintext)?;
    if plaintext.len() > LIMIT {
        return Err(reject("expanded plaintext too large"));
    }
    Ok(plaintext)
}
fn read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let file = File::open(path)?;
    if file.metadata()?.len() > limit as u64 {
        return Err(reject("file limit"));
    }
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(reject("file grew beyond limit"));
    }
    Ok(bytes)
}
fn read_object(vault: &Path, id: &str) -> Result<Vec<u8>> {
    if !object_id(id) {
        return Err(reject("invalid ciphertext id"));
    }
    let bytes = read(&vault.join(format!("{id}.age")), LIMIT + 4096)?;
    if digest(&bytes) != id {
        return Err(reject("ciphertext identity mismatch"));
    }
    Ok(bytes)
}
