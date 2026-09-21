// SPDX-License-Identifier: Apache-2.0
use vcp_domain::{accounting::*, ids::*, revision::*, workspace::*};
use vcp_lifecycle::foundation::Config;
use vcp_store::BackendKind;
pub fn config(root: &std::path::Path, workspace: &std::path::Path, backend: BackendKind) -> Config {
    let currency: Currency = "USD".to_owned().try_into().unwrap();
    Config {
        canonical_root: root.into(),
        backend,
        workspace: WorkspaceId::parse("console-workspace").unwrap(),
        session: SessionId::parse("console-session").unwrap(),
        binding: Binding {
            host: HostId::parse("native-console").unwrap(),
            root: workspace.to_string_lossy().into_owned(),
            repository: "fixture".into(),
            worktree: "main".into(),
            revision: Revision::ZERO,
        },
        actor: ActorId::parse("console-owner").unwrap(),
        root_task: TaskId::parse("console-task").unwrap(),
        cap: Money {
            currency: currency.clone(),
            micros: Micros::new(1000),
        },
        protected: Micros::ZERO,
        price: PriceSnapshot {
            id: "a".repeat(64),
            provider: "fixture".into(),
            model: "fixture".into(),
            currency,
            capability: "b".repeat(64),
            valid_until: Timestamp::new(u64::MAX),
            rates: Default::default(),
        },
        input_ceiling: Units::new(100),
        output_ceiling: Units::new(100),
        artifact_limit: ByteCount::new(16 * 1024 * 1024),
        max_transport_retries: 2,
        host_tool_denials: vec![],
    }
}
