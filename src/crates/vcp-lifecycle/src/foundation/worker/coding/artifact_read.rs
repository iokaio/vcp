// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::Value;
use std::io::Write;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    artifact: ArtifactId,
    offset: u64,
    length: u64,
}

/// Consume the verified stream without retaining bytes outside the requested range.
struct Window {
    offset: u64,
    end: u64,
    observed: u64,
    bytes: Vec<u8>,
}
impl Write for Window {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let end = self
            .observed
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| std::io::Error::other("artifact range overflow"))?;
        let start = self.observed.max(self.offset);
        let stop = end.min(self.end);
        if start < stop {
            self.bytes.extend_from_slice(
                &bytes[(start - self.observed) as usize..(stop - self.observed) as usize],
            );
        }
        self.observed = end;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Context {
    pub fn read_coding_artifact(
        &self,
        binding: &ThreadBinding,
        arguments: &str,
    ) -> Result<(Value, Option<ArtifactId>)> {
        self.require_coding_tool(binding, "vcp_artifact_read")?;
        self.tool_identity(binding, "vcp_artifact_read")?;
        // It must not provide a second route around workspace read denials.
        self.tool_identity(binding, "vcp_read")?;
        self.child_context_scope(binding)?;
        let input: Input = serde_json::from_str(arguments)?;
        let task: Task = self
            .engine
            .store()
            .state()
            .record(
                Collection::Task,
                binding.scope.task.as_str(),
                &binding.scope.workspace,
            )?
            .decode()?;
        if task.scope != binding.scope || task.redaction.is_some() {
            return Ok((
                serde_json::json!({"artifact":input.artifact,"availability":"unavailable","text":null,"complete":false}),
                None,
            ));
        }
        if input.length == 0 || input.length > 65536 {
            return Err("artifact read length must be 1..65536 bytes".into());
        }
        let end = input
            .offset
            .checked_add(input.length)
            .ok_or("artifact range overflow")?;
        let mut window = Window {
            offset: input.offset,
            end,
            observed: 0,
            bytes: Vec::new(),
        };
        let mut access = self.history_access();
        access.tasks = Some(std::collections::BTreeSet::from([binding
            .scope
            .task
            .clone()]));
        let descriptor = match vcp_audit::history::History::read_artifact(
            self.engine.store(),
            &access,
            &input.artifact,
            &mut window,
        ) {
            Ok(descriptor) => descriptor,
            Err(_) => {
                return Ok((
                    serde_json::json!({"artifact":input.artifact,"availability":"unavailable","text":null,"complete":false}),
                    None,
                ))
            }
        };
        let stop = end.min(descriptor.length.get());
        let start = input.offset.min(descriptor.length.get());
        let (encoding, value) = match std::str::from_utf8(&window.bytes) {
            Ok(text) => ("utf8", text.to_owned()),
            Err(_) => (
                "hex",
                window
                    .bytes
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>(),
            ),
        };
        let availability = if descriptor.state != vcp_domain::artifact::CaptureState::Complete {
            "partial_capture"
        } else if !descriptor.spec.omissions.is_empty() {
            "retained_with_omissions"
        } else {
            "retained"
        };
        Ok((
            serde_json::json!({"artifact":input.artifact,"availability":availability,"source_sha256":descriptor.sha256,
            "source_bytes":descriptor.length,"range":{"start":start,"end":stop},"encoding":encoding,"content":value,
            "capture_state":descriptor.state,"omissions":descriptor.spec.omissions,
            "next_offset":if stop < descriptor.length.get() { Some(stop) } else { None },"trust":"untrusted"}),
            Some(input.artifact),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_window_keeps_exact_bytes_across_stream_chunks() {
        let mut window = Window {
            offset: 3,
            end: 8,
            observed: 0,
            bytes: vec![],
        };
        window.write_all(b"ab").unwrap();
        window.write_all(b"cdef").unwrap();
        window.write_all(b"ghijkl").unwrap();
        assert_eq!(window.bytes, b"defgh");
        assert_eq!(window.observed, 12);
    }
}
