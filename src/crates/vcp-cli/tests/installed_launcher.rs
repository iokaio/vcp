// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
//! Native launcher boundaries; these synthetic installations are not release qualification.
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Fixture {
    _temp: tempfile::TempDir,
    launcher: PathBuf,
    engine: PathBuf,
    data: PathBuf,
}

impl Fixture {
    fn new(binary: &Path) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let app = temp.path().join("Program Files β");
        let data = temp.path().join("Private Data β");
        let install = app.join("engine");
        let id = "b".repeat(64);
        let release = install.join("releases").join(&id);
        fs::create_dir_all(&release).unwrap();
        let launcher = app.join("vcp.exe");
        let engine = release.join("vcp.exe");
        fs::copy(env!("CARGO_BIN_EXE_vcp-launch"), &launcher).unwrap();
        fs::copy(binary, &engine).unwrap();
        let (sha256, bytes) =
            vcp_protocol::digest_reader(fs::File::open(&engine).unwrap()).unwrap();
        for (file, value) in [
            (
                install.join(".vcp-install-owned.json"),
                json!({"schema":"vcp-install-owned/1", "install_root":install,"data_root":data}),
            ),
            (
                install.join("active.json"),
                json!({"schema":"vcp-install-pointer/1","release":id,"package_sha256":id,"data_root":data}),
            ),
            (
                release.join("manifest.json"),
                json!({"schema":"vcp-distribution-manifest/1","files":[{"path":"vcp.exe","bytes":bytes,"sha256":sha256}]}),
            ),
        ] {
            fs::write(file, serde_json::to_vec(&value).unwrap()).unwrap();
        }
        Self {
            _temp: temp,
            launcher,
            engine,
            data,
        }
    }
}

#[test]
fn stable_launch_preserves_native_arguments_output_and_exit_code() {
    let fixture = Fixture::new(Path::new(env!("CARGO_BIN_EXE_vcp")));
    let resolved = Command::new(&fixture.launcher)
        .arg("--resolve-installation")
        .output()
        .unwrap();
    assert!(
        resolved.status.success(),
        "{}",
        String::from_utf8_lossy(&resolved.stderr)
    );
    let value: Value = serde_json::from_slice(&resolved.stdout).unwrap();
    assert_eq!(value["schema"], "vcp-installed-engine/1");
    assert_eq!(
        fs::canonicalize(value["executable"].as_str().unwrap()).unwrap(),
        fs::canonicalize(&fixture.engine).unwrap()
    );
    assert_eq!(value["data_directory"], fixture.data.to_str().unwrap());
    for args in [
        vec!["--version"],
        vec!["--help"],
        vec!["--invalid-β=two words \"quoted\" \\ end"],
    ] {
        let direct = Command::new(&fixture.engine).args(&args).output().unwrap();
        let launched = Command::new(&fixture.launcher).args(args).output().unwrap();
        assert_eq!(launched.status.code(), direct.status.code());
        assert_eq!(launched.stdout, direct.stdout);
        assert_eq!(launched.stderr, direct.stderr);
    }
    let refused = Command::new(&fixture.launcher)
        .arg("local-bridge")
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("--resolve-installation"));
    fs::write(&fixture.engine, b"tampered").unwrap();
    assert_eq!(
        Command::new(&fixture.launcher)
            .arg("--version")
            .output()
            .unwrap()
            .status
            .code(),
        Some(2)
    );
}

// Test-only entry point used as a synthetic engine. Never included in a package.
#[test]
#[ignore = "invoked only by launcher containment test"]
fn launcher_child_fixture() {
    let Some(destination) = std::env::var_os("VCP_LAUNCH_TEST_READY") else {
        return;
    };
    let destination = PathBuf::from(destination);
    if std::env::var_os("VCP_LAUNCH_TEST_GRANDCHILD").is_some() {
        fs::write(
            destination.with_extension("grandchild"),
            std::process::id().to_string(),
        )
        .unwrap();
    } else {
        let _child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "launcher_child_fixture", "--ignored"])
            .env("VCP_LAUNCH_TEST_GRANDCHILD", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        fs::write(&destination, std::process::id().to_string()).unwrap();
    }
    std::thread::sleep(Duration::from_secs(120));
}

#[test]
#[ignore = "invoked only by installed data-root test"]
fn installed_data_fixture() {
    let Some(destination) = std::env::var_os("VCP_LAUNCH_TEST_DATA") else {
        return;
    };
    fs::write(
        destination,
        serde_json::to_vec(&vcp_cli::settings::default_data().unwrap()).unwrap(),
    )
    .unwrap();
}

#[test]
fn installed_engine_uses_registered_custom_data_root() {
    let fixture = Fixture::new(&std::env::current_exe().unwrap());
    let destination = fixture._temp.path().join("selected-data.json");
    let output = Command::new(&fixture.launcher)
        .args(["--exact", "installed_data_fixture", "--ignored"])
        .env("VCP_LAUNCH_TEST_DATA", &destination)
        .env(
            "LOCALAPPDATA",
            fixture._temp.path().join("different default"),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let selected: PathBuf = serde_json::from_slice(&fs::read(destination).unwrap()).unwrap();
    assert_eq!(selected, fixture.data);
}

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn killing_stable_launcher_stops_engine_and_descendants() {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };
    let fixture = Fixture::new(&std::env::current_exe().unwrap());
    let ready = fixture._temp.path().join("ready");
    let mut child = OwnedChild(
        Command::new(&fixture.launcher)
            .args(["--exact", "launcher_child_fixture", "--ignored"])
            .env("VCP_LAUNCH_TEST_READY", &ready)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ready.exists() || !ready.with_extension("grandchild").exists() {
        assert!(
            Instant::now() < deadline,
            "contained fixture did not become ready"
        );
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "launcher exited before fixture readiness"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    let handles: Vec<_> = [ready.clone(), ready.with_extension("grandchild")]
        .iter()
        .map(|file| {
            let id = fs::read_to_string(file).unwrap().parse::<u32>().unwrap();
            // SAFETY: the fixture's process ID is only used to acquire an owned,
            // synchronize-only handle before terminating its known launcher.
            let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, id) };
            assert!(!handle.is_null());
            unsafe { OwnedHandle::from_raw_handle(handle) }
        })
        .collect();
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    for handle in handles {
        assert_eq!(
            unsafe { WaitForSingleObject(handle.as_raw_handle(), 10_000) },
            0,
            "descendant survived its launcher's termination"
        );
    }
}
