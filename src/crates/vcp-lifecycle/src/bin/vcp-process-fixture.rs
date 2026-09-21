// SPDX-License-Identifier: Apache-2.0
//! Synthetic native process fixture; never used to execute user tasks.
use std::{fs::OpenOptions, io::Write, path::PathBuf, process::Command, time::Duration};

fn main() -> std::io::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mode = args
        .next()
        .ok_or_else(|| std::io::Error::other("missing mode"))?;
    let directory = PathBuf::from(
        args.next()
            .ok_or_else(|| std::io::Error::other("missing fixture directory"))?,
    );
    match mode.to_str() {
        Some("encoded-output-utf8" | "encoded-output-utf16") => {
            use std::io::Read;
            if std::io::stdin().read(&mut [0; 1])? != 0 {
                return Err(std::io::Error::other("pipe stdin was not null"));
            }
            let text = format!("raw-prefix{}\u{1b}[31mé終", "😀".repeat(20_000));
            let mut bytes = if mode == "encoded-output-utf16" {
                text.encode_utf16()
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>()
            } else {
                text.into_bytes()
            };
            bytes.push(0xff);
            std::io::stdout().write_all(&bytes)?;
            std::io::stdout().flush()?;
            std::process::exit(7);
        }

        Some("duplex-echo") => {
            use std::io::BufRead;
            let mut output = std::io::stdout().lock();
            for line in std::io::stdin().lock().lines() {
                let line = line?;
                writeln!(
                    OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(directory.join("duplex-input"))?,
                    "{line}"
                )?;
                let record = serde_json::json!({"line":line,"public":std::env::var("VCP_DUPLEX_PUBLIC").ok(),"ci":std::env::var("CI").ok(),
                    "path_inherited":std::env::var_os("PATH").is_some(),"cwd":std::env::current_dir()?});
                writeln!(output, "{record}")?;
                output.flush()?;
                eprintln!("duplex diagnostic");
            }
        }
        Some("duplex-flood") => {
            std::io::stdout().write_all(&vec![b'x'; 1024 * 1024])?;
            std::thread::sleep(Duration::from_secs(30));
        }
        Some("duplex-queue") => {
            for _ in 0..100 {
                println!("queued");
            }
            std::thread::sleep(Duration::from_secs(30));
        }
        Some("duplex-stderr") => {
            std::io::stderr().write_all(&vec![b'e'; 1024 * 1024])?;
            std::thread::sleep(Duration::from_secs(30));
        }
        Some("duplex-silent") => {
            std::fs::write(directory.join("duplex-ready"), b"ready")?;
            std::thread::sleep(Duration::from_secs(30));
        }
        Some("duplex-partial") => {
            std::io::stdout().write_all(b"unterminated")?;
        }
        Some("process-count") => {
            let _first = Command::new(std::env::current_exe()?)
                .arg("locked-child")
                .arg(&directory)
                .spawn()?;
            let until = std::time::Instant::now() + Duration::from_secs(5);
            while !directory.join("child-ready").exists() {
                if std::time::Instant::now() > until {
                    return Err(std::io::Error::other("first child did not start"));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let additional = Command::new(std::env::current_exe()?)
                .arg("write")
                .arg(directory.join("excess-marker"))
                .spawn();
            let blocked = match additional {
                Err(_) => true,
                Ok(mut child) => !child.wait()?.success(),
            };
            std::fs::write(
                directory.join("count-result"),
                if blocked { "blocked" } else { "started" },
            )?;
            while !directory.join("finish").exists() {
                if std::time::Instant::now() > until {
                    return Err(std::io::Error::other("count fixture not released"));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        Some("terminal") => {
            use std::io::IsTerminal;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let record = serde_json::json!({"stdin_terminal":std::io::stdin().is_terminal(),
                "stdout_terminal":std::io::stdout().is_terminal(),"input":input});
            std::fs::write(
                directory.join("terminal.json"),
                serde_json::to_vec(&record)?,
            )?;
            println!("terminal stdout");
            eprintln!("terminal stderr");
        }
        Some("orphan") => {
            let _child = Command::new(std::env::current_exe()?)
                .arg("locked-child")
                .arg(&directory)
                .spawn()?;
            let until = std::time::Instant::now() + Duration::from_secs(5);
            while !directory.join("child-ready").exists() {
                if std::time::Instant::now() > until {
                    return Err(std::io::Error::other("descendant did not start"));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        Some("verify") => {
            let content = std::fs::read(directory.join("fixture.txt"))?;
            if content != b"answer = 42\n" {
                return Err(std::io::Error::other("fixture assertion failed"));
            }
            println!("fixture assertion passed: answer = 42");
        }
        Some("tree") => {
            let child = Command::new(std::env::current_exe()?)
                .arg("locked-child")
                .arg(&directory)
                .spawn()?;
            std::fs::write(directory.join("parent-ready"), child.id().to_string())?;
            loop {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        Some("locked-child") => {
            let mut options = OpenOptions::new();
            options.create(true).truncate(false).write(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options.share_mode(0);
            }
            let _lock = options.open(directory.join("locked"))?;
            std::fs::write(
                directory.join("child-ready"),
                std::process::id().to_string(),
            )?;
            loop {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        Some("flood") => {
            let bytes = [b'x'; 8192];
            for _ in 0..256 {
                std::io::stdout().write_all(&bytes)?;
                std::io::stderr().write_all(&bytes)?;
            }
        }
        Some("argv") => {
            let arguments: Vec<_> = args.map(|arg| arg.to_string_lossy().into_owned()).collect();
            let record = serde_json::json!({
                "args": arguments,
                "cwd": std::env::current_dir()?,
                "fixture_env": std::env::var("VCP_FIXTURE_VALUE").ok(),
                "inherited": std::env::var("VCP_FIXTURE_MUST_NOT_INHERIT").ok(),
                "ci": std::env::var("CI").ok(),
            });
            std::fs::write(directory.join("argv.json"), serde_json::to_vec(&record)?)?;
            std::io::stdout().write_all(b"one\r\ntwo\r\n")?;
        }
        Some("write") => {
            std::fs::write(directory, b"fixture effect\r\n")?;
        }
        Some("boundary") => {
            let outside = PathBuf::from(
                args.next()
                    .ok_or_else(|| std::io::Error::other("missing outside root"))?,
            );
            let endpoint: std::net::SocketAddr = args
                .next()
                .ok_or_else(|| std::io::Error::other("missing endpoint"))?
                .to_string_lossy()
                .parse()
                .map_err(std::io::Error::other)?;
            let nonce = args
                .next()
                .ok_or_else(|| std::io::Error::other("missing nonce"))?;
            let inside_result = std::fs::write(directory.join("inside.txt"), b"inside");
            let inside_error = inside_result
                .as_ref()
                .err()
                .and_then(|error| error.raw_os_error());
            let inside_write = inside_result.is_ok();
            let outside_read = std::fs::read(outside.join("read-canary.txt")).is_ok();
            let outside_result = std::fs::write(outside.join("write-canary.txt"), b"outside");
            let outside_error = outside_result
                .as_ref()
                .err()
                .and_then(|error| error.raw_os_error());
            let outside_write = outside_result.is_ok();
            let junction_write = std::fs::write(
                directory.join("redirect").join("junction-canary.txt"),
                b"redirected",
            )
            .is_ok();
            let network = std::net::TcpStream::connect_timeout(&endpoint, Duration::from_secs(2))
                .and_then(|mut stream| stream.write_all(nonce.to_string_lossy().as_bytes()))
                .is_ok();
            println!(
                "{}",
                serde_json::json!({"inside_write":inside_write,"outside_read":outside_read,
                "outside_write":outside_write,"junction_write":junction_write,"network":network,
                "inside_error":inside_error,"outside_error":outside_error})
            );
        }
        _ => return Err(std::io::Error::other("unknown fixture mode")),
    }
    Ok(())
}
