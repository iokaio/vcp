// SPDX-License-Identifier: Apache-2.0
//! External marker fixture for native hook qualification.
use std::{
    fs::OpenOptions,
    io::{BufRead, Write},
    path::PathBuf,
    time::Duration,
};

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mode = &args[1];
    let marker = PathBuf::from(&args[2]);
    let line = std::io::stdin().lock().lines().next().unwrap().unwrap();
    assert_eq!(args[3], "--vcp-hook-input-sha256");
    assert_eq!(args[4], vcp_protocol::digest_bytes(line.as_bytes()));
    let plan: serde_json::Value = serde_json::from_str(&line).unwrap();
    #[cfg(windows)]
    let _native_liveness = if mode == "timeout" {
        use std::os::windows::fs::OpenOptionsExt;
        Some(
            OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .share_mode(0)
                .open(marker.with_extension("lock"))
                .unwrap(),
        )
    } else {
        None
    };
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&marker)
        .unwrap();
    writeln!(file, "{}", plan["definition"]["id"].as_str().unwrap()).unwrap();
    file.sync_all().unwrap();
    if mode == "edit_source" {
        std::fs::write(
            marker.parent().unwrap().join("observed.txt"),
            "changed by hook",
        )
        .unwrap();
    }
    if mode == "edit_pinned" {
        std::fs::write(
            marker.parent().unwrap().join("input.txt"),
            "source changed by later hook",
        )
        .unwrap();
    }
    if mode == "timeout" {
        std::thread::sleep(Duration::from_secs(60));
    }
    if mode == "malformed" {
        println!("this is not a result");
        return;
    }
    if mode == "malformed_failure" {
        println!("this failed notification also violates the output schema");
        std::io::stdout().flush().unwrap();
        std::process::exit(1);
    }
    if mode == "notification_failure" {
        std::process::exit(1);
    }
    if mode == "overflow" {
        println!("{}", "x".repeat(128 * 1024));
        return;
    }
    let findings = if mode == "environment" {
        vec![
            format!(
                "ambient={}",
                std::env::var_os("VCP_HOOK_SECRET_CANARY").is_some()
            ),
            format!("ci={}", std::env::var("CI").unwrap_or_default()),
        ]
    } else {
        vec![]
    };
    let rewrite = if mode == "rewrite" {
        plan["input"]["payload"]["rewrite"].clone()
    } else if mode == "rewrite_read" {
        serde_json::json!({"original_digest":plan["input"]["payload"]["operation_digest"],
            "tool":"vcp_read","arguments":{"path":"rewritten.txt","max_bytes":1024,"start_line":null,"end_line":null}})
    } else {
        serde_json::Value::Null
    };
    let context = if mode == "context" {
        serde_json::json!({"text":"NATIVE_HOOK_CONTEXT_PROVENANCE","artifact_refs":[]})
    } else {
        serde_json::Value::Null
    };
    println!(
        "{}",
        serde_json::json!({"schema_version":1,"findings":findings,"context":context,"rewrite":rewrite,"block":false})
    );
}
