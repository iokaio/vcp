// SPDX-License-Identifier: Apache-2.0
//! Credential headers and recovery material have no serialization path into a
//! captured body. This is structural separation, not a claim to detect secrets
//! intentionally supplied within arbitrary user/model content.
use crate::{Error, Result};
use vcp_domain::artifact::{ArtifactDescriptor, ArtifactSpec, Channel, Omission};
use vcp_store::{
    artifact::{ArtifactWriter, LocalWriter, CHUNK_BYTES},
    Store,
};

/// Does not implement Serialize, Display, or expose bytes to capture inputs.
pub struct ProviderCredential(String);
impl std::fmt::Debug for ProviderCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProviderCredential([omitted])")
    }
}
impl ProviderCredential {
    pub fn from_config(value: String) -> Self {
        Self(value)
    }
    /// Only the transport adapter calls this method when constructing its header.
    pub fn header_for_transport(&self) -> &str {
        &self.0
    }
}
pub struct RequestBody(serde_json::Value);
impl RequestBody {
    pub fn new(body: serde_json::Value) -> Self {
        Self(body)
    }
}
pub struct CapturedRequest {
    pub descriptor: ArtifactDescriptor,
    body: Vec<u8>,
}
impl CapturedRequest {
    pub fn body_for_transport(&self) -> &[u8] {
        &self.body
    }
}
pub struct CaptureSession<'a> {
    store: &'a Store,
    failed: bool,
}
impl<'a> CaptureSession<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self {
            store,
            // Interrupted captures are discovered even when the last failure
            // could not be written to the canonical log. The host must reconcile
            // them before creating a new dependent capture session.
            failed: store
                .spool()
                .unfinished()
                .map(|rows| {
                    rows.iter()
                        .any(|row| row.state == vcp_domain::artifact::CaptureState::Pending)
                })
                .unwrap_or(true),
        }
    }
    pub fn request(
        &mut self,
        mut spec: ArtifactSpec,
        body: &RequestBody,
    ) -> Result<CapturedRequest> {
        if self.failed {
            return Err(Error::Host);
        }
        spec.channel = Channel::RequestBody;
        spec.omissions
            .extend([Omission::AuthenticationHeaders, Omission::RecoveryMaterial]);
        let bytes = serde_json::to_vec(&body.0)?;
        let result = (|| -> Result<ArtifactDescriptor> {
            let mut writer = self.store.spool().create(spec)?;
            for chunk in bytes.chunks(CHUNK_BYTES) {
                writer.write_chunk(chunk)?;
            }
            Ok(writer.finalize()?)
        })();
        match result {
            Ok(descriptor) => Ok(CapturedRequest {
                descriptor,
                body: bytes,
            }),
            Err(error) => {
                self.failed = true;
                Err(error)
            }
        }
    }
    pub fn stream(&mut self, spec: ArtifactSpec) -> Result<CapturedStream<'_>> {
        if self.failed {
            return Err(Error::Host);
        }
        let writer = match self.store.spool().create(spec) {
            Ok(writer) => writer,
            Err(error) => {
                self.failed = true;
                return Err(error.into());
            }
        };
        Ok(CapturedStream {
            writer,
            failed: &mut self.failed,
            finalized: false,
        })
    }
    pub fn dispatch_allowed(&self) -> bool {
        !self.failed && self.store.healthy()
    }
}
pub struct CapturedStream<'a> {
    writer: LocalWriter,
    failed: &'a mut bool,
    finalized: bool,
}
impl CapturedStream<'_> {
    pub fn write(&mut self, bytes: &[u8]) -> Result<()> {
        if *self.failed {
            return Err(Error::Host);
        }
        for chunk in bytes.chunks(CHUNK_BYTES) {
            if let Err(error) = self.writer.write_chunk(chunk) {
                *self.failed = true;
                return Err(error.into());
            }
        }
        Ok(())
    }
    pub fn finalize(mut self) -> Result<ArtifactDescriptor> {
        match self.writer.finalize() {
            Ok(value) => {
                self.finalized = true;
                Ok(value)
            }
            Err(error) => {
                *self.failed = true;
                Err(error.into())
            }
        }
    }
}
impl Drop for CapturedStream<'_> {
    fn drop(&mut self) {
        if !self.finalized {
            *self.failed = true;
        }
    }
}
