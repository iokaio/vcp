// SPDX-License-Identifier: Apache-2.0
//! The existing resumable job's streamed archive representation. Its source is
//! an actual pinned Snapshot; persisted descriptors never substitute for it.
use super::*;
use crate::{history_index::io::Files, portable_snapshot::wire, vault_crypto::stream};
use std::io::Read;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Descriptor {
    pub(super) root: stream::RootDescriptor,
    pub(super) payload: Object,
}
impl Descriptor {
    pub(super) fn validate(&self) -> Result<()> {
        let hash = |value: &str| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if self.root.format != wire::FORMAT
            || !hash(&self.root.sha256)
            || self.root.descriptor_bytes == 0
            || self.root.descriptor_bytes > 128 * 1024
            || !hash(&self.payload.sha256)
            || self.payload.bytes == 0
        {
            return Err(Error::Corruption("snapshot stream descriptor"));
        }
        stream::StreamLimits::for_payload(self.payload.bytes)?;
        Ok(())
    }
}
impl Jobs {
    pub(super) async fn prepare_stream(
        &self,
        mut capture: Capture,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Prepared> {
        self.owns(&capture.job)?;
        let check = || {
            if cancelled() {
                Err(Error::Unavailable("snapshot preparation cancelled"))
            } else {
                Ok(())
            }
        };
        check()?;
        // Every attempt has a fresh private scratch directory. Failed attempts
        // remain recovery residue; only a synced, fully hashed wire is published.
        let attempt = self.path(
            &capture.job.id,
            &format!("archive-{}", TransactionId::new()),
        )?;
        fs::create_dir(&attempt)?;
        let attempt = Directory::open(&attempt, &self.forbidden)?;
        let scratch_path = attempt.path.join("scratch");
        let closure_path = attempt.path.join("closure");
        fs::create_dir(&scratch_path)?;
        fs::create_dir(&closure_path)?;
        let scratch = Directory::open(&scratch_path, &self.forbidden)?;
        let closure = Directory::open(&closure_path, &self.forbidden)?;
        let mut scratch = Files::new(&scratch);
        let mut closure = Files::new(&closure);
        let source = capture.snapshot.logical_digest().await?;
        if source != capture.job.state_digest
            || capture.snapshot.current().watermark != capture.job.watermark
        {
            return Err(Error::Conflict("snapshot stream source differs"));
        }
        let archive = capture
            .snapshot
            .capture_stream(
                &mut scratch,
                &capture.job.workspace,
                &capture.job.inputs,
                &check,
            )
            .await?;
        let objects = archive.collect(&mut scratch, &mut closure, &check).await?;
        let pending = attempt.path.join("archive.pending");
        let mut output = private_paths::create_private(&pending)?;
        let (root, payload) = wire::write(
            &mut closure,
            &objects,
            &archive.root()?,
            &mut output,
            u64::MAX,
            &check,
        )
        .await?;
        output.sync_all()?;
        drop(output);
        let descriptor = Descriptor { root, payload };
        descriptor.validate()?;
        check()?;
        let path = self.path(&capture.job.id, "archive")?;
        if path.exists() {
            verify_file(&path, &descriptor.payload, &check)?;
        } else {
            fs::hard_link(&pending, &path)?;
        }
        check()?;
        Ok(Prepared {
            job: capture.job.id,
            revision: capture.job.revision,
            inventory: descriptor.root.sha256.clone(),
            digest: descriptor.payload.sha256.clone(),
            source,
            inputs: digest_bytes(&canonical_bytes(&capture.job.inputs)?),
            stream: Some(descriptor),
        })
    }
    pub(super) fn encrypt_stream(
        &self,
        job: &Job,
        trust: &LocalTrust,
        keys: &VerifiedKeys,
        staging: &PrivateStaging,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Encrypted> {
        self.owns(job)?;
        let descriptor = job
            .stream
            .as_ref()
            .ok_or(Error::Corruption("snapshot stream missing"))?;
        descriptor.validate()?;
        if job.inventory.as_ref() != Some(&descriptor.root.sha256)
            || job.archive_digest.as_ref() != Some(&descriptor.payload.sha256)
        {
            return Err(Error::Corruption("snapshot stream identity differs"));
        }
        let checkpoint = &trust.configuration().checkpoint;
        let manifest = stream::StreamManifest {
            format: stream::FORMAT.into(),
            workspace: job.workspace.clone(),
            lineage: trust.configuration().lineage.clone(),
            sequence: checkpoint
                .sequence
                .checked_add(1)
                .ok_or(Error::Limit("snapshot sequence"))?,
            deletion: job.deletion,
            parent: checkpoint.parent.clone(),
            archive_root: descriptor.root.clone(),
            payload: descriptor.payload.clone(),
        };
        let check = || {
            if cancelled() {
                Err(Error::Unavailable("snapshot encryption cancelled"))
            } else {
                Ok(())
            }
        };
        self.encrypt_owned(job, keys, manifest.clone().into(), cancelled, || {
            let mut input = private_paths::PublicCiphertext::open_stream(
                &self.path(&job.id, "archive")?,
                descriptor.payload.bytes,
            )?;
            let output = trust.encrypt_stream(
                keys,
                staging,
                manifest,
                &mut input,
                job.trust_revision,
                stream::StreamLimits::for_payload(descriptor.payload.bytes)?,
                &check,
            )?;
            input.finish()?;
            Ok(output)
        })
    }
}
fn verify_file(path: &Path, expected: &Object, check: &dyn Fn() -> Result<()>) -> Result<()> {
    use sha2::{Digest, Sha256};
    let mut input = private_paths::PublicCiphertext::open_stream(path, expected.bytes)?;
    let mut digest = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0; 65536];
    loop {
        check()?;
        let used = input.read(&mut buffer)?;
        if used == 0 {
            break;
        }
        count = count
            .checked_add(used as u64)
            .ok_or(Error::Limit("snapshot stream size"))?;
        digest.update(&buffer[..used]);
    }
    input.finish()?;
    if count != expected.bytes || format!("{:x}", digest.finalize()) != expected.sha256 {
        return Err(Error::Conflict("owned snapshot archive differs"));
    }
    Ok(())
}
