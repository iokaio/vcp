// SPDX-License-Identifier: Apache-2.0
use serde::{Deserialize, Deserializer, Serialize};

macro_rules! ids {
    ($($name:ident),+ $(,)?) => {$ (
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);
        impl $name {
            pub fn new() -> Self { Self(uuid::Uuid::new_v4().to_string()) }
            /// Explicit IDs support import and synthetic fixtures. IDs never confer access.
            pub fn parse(value: impl Into<String>) -> crate::Result<Self> {
                let value = value.into();
                if value.is_empty() || value.len() > 96 || !value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
                    return Err(crate::Error::Invalid("opaque ID"));
                }
                Ok(Self(value))
            }
            pub fn as_str(&self) -> &str { &self.0 }
        }
        impl Default for $name { fn default() -> Self { Self::new() } }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
                Self::parse(String::deserialize(d)?).map_err(serde::de::Error::custom)
            }
        }
    )+};
}

ids!(
    WorkspaceId,
    SessionId,
    TaskId,
    TurnId,
    AgentId,
    StepId,
    AttemptId,
    ToolRunId,
    ExecutionId,
    ArtifactId,
    ApprovalId,
    VerificationId,
    EventId,
    CommandId,
    TransactionId,
    ActorId,
    ControllerId,
    HostId,
    ReservationId,
    ObservationId,
    ClaimId,
    GenerationId,
    SnapshotId
);
