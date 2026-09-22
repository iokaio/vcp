// SPDX-License-Identifier: Apache-2.0
//! Qualification-only PTY byte transport. The caller owns frozen run admission,
//! canonical progress checks and accounting. No credentials are written to output.
use codex_utils_pty::{spawn_pty_process, TerminalSize};
use serde::Deserialize;
use std::{
    collections::HashMap,
    io::{BufRead, Write},
    path::PathBuf,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    executable: PathBuf,
    workspace: PathBuf,
    arguments: Vec<String>,
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Control {
    Write { text: String },
    Terminate,
}
fn emit(value: serde_json::Value) -> std::io::Result<()> {
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &value)?;
    out.write_all(b"\n")?;
    out.flush()
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let spec: Spec = serde_json::from_slice(&std::fs::read(
        args.next().ok_or("one transport spec required")?,
    )?)?;
    if args.next().is_some() || spec.arguments.len() > 64 {
        return Err("invalid driver arguments".into());
    }
    let mut environment = HashMap::new();
    for key in [
        "SystemRoot",
        "WINDIR",
        "PATH",
        "TEMP",
        "TMP",
        "LOCALAPPDATA",
        "USERPROFILE",
        "OPENROUTER_API_KEY",
    ] {
        if let Ok(value) = std::env::var(key) {
            environment.insert(key.to_owned(), value);
        }
    }
    let mut child = spawn_pty_process(
        spec.executable
            .to_str()
            .ok_or("Unicode executable required")?,
        &spec.arguments,
        &spec.workspace,
        &environment,
        &None,
        TerminalSize {
            rows: 40,
            cols: 140,
        },
        &[],
    )
    .await?;
    let writer = child.session.writer_sender();
    let (send, mut controls) = tokio::sync::mpsc::channel(4);
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let control = line
                .ok()
                .filter(|line| line.len() <= 16 * 1024)
                .and_then(|line| serde_json::from_str::<Control>(&line).ok());
            if send.blocking_send(control).is_err() {
                return;
            }
        }
        let _ = send.blocking_send(None);
    });
    emit(serde_json::json!({"type":"started"}))?;
    let mut total = 0usize;
    loop {
        tokio::select! {
            control=controls.recv()=>match control.flatten() {
                Some(Control::Write { text }) if text.len()<=8192 => { writer.send(text.into_bytes()).await?; }
                _ => {child.session.terminate();}
            },
            bytes=child.stdout_rx.recv()=>if let Some(bytes)=bytes {
                total=total.checked_add(bytes.len()).ok_or("output overflow")?;
                if total>16*1024*1024 {child.session.terminate();return Err("PTY transcript exceeds qualification bound".into());}
                if emit(serde_json::json!({"type":"output","text":String::from_utf8_lossy(&bytes)})).is_err() { child.session.terminate();return Err("qualification consumer lost".into()); }
            },
            code=&mut child.exit_rx=>{
                emit(serde_json::json!({"type":"exit","code":code?}))?;
                break;
            }
        }
    }
    Ok(())
}
