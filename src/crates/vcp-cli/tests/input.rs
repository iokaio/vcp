// SPDX-License-Identifier: Apache-2.0
use std::io::{self, BufReader, Cursor};
use vcp_cli::input::read_command;
use vcp_domain::{ids::*, revision::*, task::TaskState};
use vcp_protocol::{
    command::{Command, CommandEnvelope},
    version::MAX_COMMAND_BYTES,
};

fn command() -> CommandEnvelope {
    CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: WorkspaceId::new(),
        session: SessionId::new(),
        task: Some(TaskId::new()),
        caller: ActorId::new(),
        controller: ControllerId::new(),
        owner_epoch: OwnerEpoch::new(1),
        expected: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        payload: Command::Transition {
            next: TaskState::Paused,
            reason: "operator pause".into(),
            verification: None,
        },
    }
}

#[test]
fn fragmented_multiple_commands_preserve_identity_and_framing() {
    let first = command();
    let second = command();
    let bytes = format!(
        "{}\r\n{}\n",
        serde_json::to_string(&first).unwrap(),
        serde_json::to_string(&second).unwrap()
    );
    let mut input = BufReader::with_capacity(3, Cursor::new(bytes));
    assert_eq!(read_command(&mut input).unwrap(), Some(first));
    assert_eq!(read_command(&mut input).unwrap(), Some(second));
    assert_eq!(read_command(&mut input).unwrap(), None);
}

#[test]
fn truncated_malformed_and_oversized_controls_never_execute() {
    let bytes = serde_json::to_vec(&command()).unwrap();
    assert_eq!(
        read_command(&mut Cursor::new(bytes)).unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );
    for bytes in [
        b"\n".to_vec(),
        b"{}\n".to_vec(),
        b"\xff\n".to_vec(),
        vec![b'x'; MAX_COMMAND_BYTES + 1],
    ] {
        assert_eq!(
            read_command(&mut Cursor::new(bytes)).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
    let mut value = serde_json::to_value(command()).unwrap();
    value["version"] = 2.into();
    assert!(read_command(&mut Cursor::new(format!("{value}\n"))).is_err());
    value["version"] = 1.into();
    value["unexpected"] = true.into();
    assert!(read_command(&mut Cursor::new(format!("{value}\n"))).is_err());
}
