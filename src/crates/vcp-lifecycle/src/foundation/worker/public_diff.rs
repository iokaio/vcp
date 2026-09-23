// SPDX-License-Identifier: Apache-2.0
//! Producer prerequisite for typed public diff reads. Hook into proposal before
//! its initial Validated fact; do not attach this to a later execution outcome.
use super::*;
use std::io::{self, Write};
use vcp_domain::public_diff::{self, Content, Disposition, Document, FileChange};

impl Context {
    pub(super) fn capture_public_diff(
        &mut self,
        binding: &ThreadBinding,
        change: ToolRunId,
        prepared: ArtifactId,
        operation: &vcp_policy::Prepared,
        changes: &[vcp_tools::patch::Change],
    ) -> Result<Option<ArtifactDescriptor>> {
        if changes.is_empty() {
            return Ok(None);
        }
        self.can_start(binding)?;
        if operation.operation().scope != binding.scope || changes.len() > public_diff::MAX_FILES {
            return Err("public diff source scope or count mismatch".into());
        }
        let files = changes
            .iter()
            .map(|change| {
                let before = match (&change.before, &change.before_bytes) {
                    (None, None) => None,
                    (Some(proof), Some(bytes))
                        if proof.path == change.path
                            && proof.bytes.get() == bytes.len() as u64
                            && proof.sha256 == vcp_protocol::digest_bytes(bytes) =>
                    {
                        Some(Content {
                            sha256: proof.sha256.clone(),
                            bytes: bytes.clone(),
                        })
                    }
                    _ => return Err("public diff before content mismatch"),
                };
                Ok(FileChange {
                    path: change.path.clone(),
                    rename_to: change.rename_to.clone(),
                    before,
                    after: change.after.as_ref().map(|bytes| Content {
                        sha256: vcp_protocol::digest_bytes(bytes),
                        bytes: bytes.clone(),
                    }),
                })
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let document = Document {
            schema: public_diff::SCHEMA.into(),
            scope: binding.scope.clone(),
            change,
            operation_digest: operation.digest().into(),
            prepared,
            disposition: Disposition::Proposed,
            files,
        };
        document.validate()?;
        let mut writer = self.engine.store().spool().create(self.spec(
            &binding.scope,
            Channel::Evidence,
            public_diff::SCHEMA,
        ))?;
        {
            let mut sink = io::BufWriter::with_capacity(
                vcp_store::artifact::CHUNK_BYTES,
                ChunkSink(&mut writer),
            );
            serde_json::to_writer(&mut sink, &document)?;
            sink.flush()?;
        }
        let descriptor = writer.finalize()?;
        self.command(
            Command::AttachArtifact {
                descriptor: descriptor.clone(),
            },
            Some(binding.scope.task.clone()),
            Revision::ZERO,
        )?;
        Ok(Some(descriptor))
    }
}
struct ChunkSink<'a>(&'a mut LocalWriter);
impl Write for ChunkSink<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        for chunk in bytes.chunks(vcp_store::artifact::CHUNK_BYTES) {
            self.0
                .write_chunk(chunk)
                .map_err(|_| io::Error::other("public diff capture failed"))?;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
