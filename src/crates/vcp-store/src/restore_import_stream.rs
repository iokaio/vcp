// SPDX-License-Identifier: Apache-2.0
//! Native import of an already semantically admitted neutral-history/2 archive.
//! Original bodies enter the existing preparation/publication path unchanged;
//! destination sanitization and an independent cold reopen remain mandatory.
use super::*;
use crate::{durable_owner::Outcome, history_index::io::Files};

pub(super) async fn prepare_stream(
    validated: &crate::restore_stage::stream::ValidatedStream,
    source_manifest: &str,
    root: &Path,
    forbidden: &[PathBuf],
    backend: BackendKind,
    operation: &CommandId,
    actor: &ActorId,
    timestamp: Timestamp,
    cancelled: &dyn Fn() -> bool,
) -> Result<Imported> {
    let check = || {
        if cancelled() {
            Err(Error::Unavailable("restore import cancelled"))
        } else {
            Ok(())
        }
    };
    check()?;
    let source = &validated.restored.owner;
    let workspace = validated.proof.manifest().workspace.clone();
    let transaction = crate::restore_authority::transaction(
        source.semantic().current(),
        &workspace,
        operation,
        actor,
        timestamp,
    )?;
    let mut pages = Files::new(&validated.replayed);
    let Outcome::Prepared(prepared) = source.prepare(&mut pages, &transaction).await? else {
        return Err(Error::Conflict(
            "restore operation already occurs in source",
        ));
    };
    let expected = source
        .advance(&mut pages, &prepared, &canonical_bytes(prepared.commit())?)
        .await?;
    let expected_digest = expected
        .semantic()
        .catalog()
        .legacy_digest(&mut pages, expected.semantic().current().into())
        .await?;
    check()?;
    if !root.exists() {
        fs::create_dir(root)?;
    }
    let directory = Directory::open(root, forbidden)?;
    // A retained legacy prefix is copied exactly, including its original source
    // commitment. Genesis archives never manufacture a whole-State replay base.
    if let Some(blob) = source.originals().base_blob() {
        let bytes = crate::history_blob::read_bounded(
            &mut pages,
            blob,
            crate::contract::MAX_STATE_BYTES + 1024 * 1024,
        )
        .await?;
        let base = crate::replay_base::ReplayBase::decode(&bytes)?;
        source
            .originals()
            .verify_base(&mut pages, Some(&base))
            .await?;
        immutable(&root.join("replay-base.json"), &bytes)?;
        immutable(
            &root.join("replay-base.seal"),
            digest_bytes(&bytes).as_bytes(),
        )?;
        if !root.join("format.json").exists() {
            immutable(
                &root.join("format.json"),
                &canonical_bytes(&serde_json::json!({"version":2,"backend":backend}))?,
            )?;
        }
    }
    let staged = validated
        .restored
        .stage_artifacts(
            &mut Files::new(&validated.source),
            &root.join("spool"),
            forbidden,
            &check,
        )
        .await?;
    drop(staged);
    if !root.join("format.json").exists() {
        immutable(
            &root.join("format.json"),
            &canonical_bytes(&serde_json::json!({
                "version": if source.originals().base_blob().is_some() { 2 } else { 1 },
                "backend": backend,
            }))?,
        )?;
    }
    check()?;
    let mut store = Store::open(root, backend, forbidden).await?;
    let imported = async {
        let mut watermark = source.originals().base_watermark();
        while watermark < source.originals().watermark() {
            check()?;
            watermark = watermark.next()?;
            let original = source
                .originals()
                .get(&mut pages, watermark)
                .await?
                .ok_or(Error::Corruption("restore original commit missing"))?;
            let receipt = store.transact_original(&original.bytes).await?;
            if receipt != original.commit.receipt {
                return Err(Error::Corruption("restore original receipt differs"));
            }
        }
        check()?;
        store.transact(transaction).await?;
        if store.current().watermark != expected.semantic().current().watermark
            || &*store.current_state() != expected.semantic().current()
            || store.prefix_digest(store.current().watermark).await? != expected_digest
        {
            return Err(Error::Corruption(
                "restore sanitized current or history differs",
            ));
        }
        Ok(())
    }
    .await;
    let closed = store.close().await;
    imported?;
    closed?;
    check()?;
    let reopened = Store::open(root, backend, forbidden).await?;
    let verified = async {
        if &*reopened.current_state() != expected.semantic().current()
            || reopened.prefix_digest(reopened.current().watermark).await? != expected_digest
        {
            return Err(Error::Corruption("restore fresh reopen differs"));
        }
        Ok(())
    }
    .await;
    let closed = reopened.close().await;
    verified?;
    closed?;
    Ok(Imported {
        root: root.to_owned(),
        workspace,
        state_digest: expected_digest,
        source_manifest: source_manifest.to_owned(),
        checkpoint: validated.restored.inputs.checkpoint.clone(),
        backend,
        forbidden: forbidden.to_vec(),
        _directory: directory,
    })
}
