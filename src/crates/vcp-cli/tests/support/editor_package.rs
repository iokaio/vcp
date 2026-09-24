// SPDX-License-Identifier: Apache-2.0
//! Actual VSIX installation and native distribution relocation, qualification only.
#![allow(dead_code)]
use serde_json::Value;
use std::{
    fs,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};
const HIDDEN: u32 = 0x08000000;
pub fn native(path: &Path) -> String {
    path.to_string_lossy()
        .strip_prefix(r"\\?\")
        .unwrap_or(&path.to_string_lossy())
        .to_owned()
}
pub fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        assert!(!kind.is_symlink());
        let destination = target.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &destination)
        } else {
            assert!(kind.is_file());
            fs::copy(entry.path(), destination).unwrap();
        }
    }
}
pub fn executable(name: &str) -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|root| root.join(name))
        .find(|path| path.is_file())
        .unwrap()
}
pub fn restricted_path() -> String {
    format!(
        "{}\\System32;{}",
        std::env::var("SystemRoot").unwrap(),
        std::env::var("SystemRoot").unwrap()
    )
}
/// Freeze archive+manifest inputs once for the entire two-backend test process.
pub fn pin_inputs(root: &Path) {
    let mut engine_hash: Option<String> = None;
    for (variable, name) in [
        ("VCP_TEST_VSIX", "candidate"),
        ("VCP_TEST_VSIX_UPDATE", "successor"),
    ] {
        let Some(value) = std::env::var_os(variable) else {
            continue;
        };
        let source = PathBuf::from(value);
        let target = root.join(name);
        fs::create_dir_all(&target).unwrap();
        let archive = target.join(source.file_name().unwrap());
        fs::copy(&source, &archive).unwrap();
        fs::copy(
            source.parent().unwrap().join("manifest.json"),
            target.join("manifest.json"),
        )
        .unwrap();
        let manifest: Value =
            serde_json::from_slice(&fs::read(target.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest["schema"], "vcp-vsix-package/1");
        let current = manifest["engine"]["executable_sha256"].as_str().unwrap();
        if let Some(expected) = &engine_hash {
            assert_eq!(expected, current)
        } else {
            engine_hash = Some(current.to_owned())
        }
        let hash = vcp_protocol::digest_bytes(&fs::read(&archive).unwrap());
        assert_eq!(manifest["archive"]["sha256"], hash);
        eprintln!("{variable} snapshot SHA256 {hash}");
        std::env::set_var(variable, archive);
    }
}
pub struct Package {
    pub vsix: PathBuf,
    pub engine: PathBuf,
    pub pwsh: PathBuf,
    pub root: PathBuf,
}
impl Package {
    pub fn prepare(root: &Path) -> Self {
        fs::create_dir_all(root).unwrap();
        let source = PathBuf::from(
            std::env::var_os("VCP_TEST_NATIVE_PACKAGE")
                .expect("verified native distribution directory required"),
        );
        let verified=Command::new(std::env::var_os("VCP_TEST_NODE").expect("pinned Node required"))
            .args(["-e", "const fs=require('node:fs'),path=require('node:path');const root=process.argv[1];require(path.join(root,'tools/package-inventory.cjs')).verifyManifest(root,JSON.parse(fs.readFileSync(path.join(root,'manifest.json'),'utf8')))"])
            .arg(&source).output().unwrap();
        assert!(
            verified.status.success(),
            "native distribution inventory failed"
        );
        let engine_dir = root.join("native-release-a");
        copy_tree(&source, &engine_dir);
        let engine = engine_dir.join("vcp.exe");
        assert_eq!(
            vcp_protocol::digest_bytes(&fs::read(&engine).unwrap()),
            vcp_protocol::digest_bytes(&fs::read(env!("CARGO_BIN_EXE_vcp")).unwrap()),
            "package engine differs from current qualified build"
        );
        let source_vsix =
            PathBuf::from(std::env::var_os("VCP_TEST_VSIX").expect("actual VSIX required"));
        let vsix = root.join("candidate.vsix");
        fs::copy(&source_vsix, &vsix).unwrap();
        let manifest = source_vsix.parent().unwrap().join("manifest.json");
        assert!(manifest.is_file(), "VSIX provenance manifest required");
        let metadata: Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
        assert_eq!(metadata["schema"], "vcp-vsix-package/1");
        assert_eq!(
            metadata["archive"]["sha256"],
            vcp_protocol::digest_bytes(&fs::read(&vsix).unwrap())
        );
        assert_eq!(
            metadata["engine"]["executable_sha256"],
            vcp_protocol::digest_bytes(&fs::read(&engine).unwrap())
        );
        fs::copy(manifest, root.join("vsix-manifest.json")).unwrap();
        Self {
            vsix,
            engine,
            pwsh: executable("pwsh.exe"),
            root: root.to_owned(),
        }
    }
    pub fn editor_cli(&self, code: &Path, user: &Path, extensions: &Path, args: &[&str]) -> Output {
        let parent = code.parent().unwrap();
        let mut roots = vec![parent.to_owned()];
        roots.extend(
            fs::read_dir(parent)
                .unwrap()
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.is_dir()),
        );
        let roots: Vec<_> = roots
            .into_iter()
            .filter(|path| {
                let file = path.join("resources/app/package.json");
                fs::read(file)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                    .is_some_and(|value| value["version"] == "1.138.0")
            })
            .collect();
        assert_eq!(roots.len(), 1, "one exact pinned runtime required");
        let runtime = &roots[0];
        let cli = runtime.join("resources/app/out/cli.js");
        let mut command = Command::new(code);
        command
            .creation_flags(HIDDEN)
            .env("ELECTRON_RUN_AS_NODE", "1")
            .env_remove("VSCODE_DEV")
            .env("PATH", format!("{};{}", native(runtime), restricted_path()))
            .current_dir(&self.root)
            .arg(cli)
            .arg("--user-data-dir")
            .arg(user)
            .arg("--extensions-dir")
            .arg(extensions)
            .args(args);
        let logs = tempfile::tempdir_in(&self.root).unwrap();
        let stdout = logs.path().join("stdout");
        let stderr = logs.path().join("stderr");
        command
            .stdout(fs::File::create(&stdout).unwrap())
            .stderr(fs::File::create(&stderr).unwrap());
        let mut child = command.spawn().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if std::time::Instant::now() >= deadline
                || fs::metadata(&stdout).unwrap().len() + fs::metadata(&stderr).unwrap().len()
                    > 1024 * 1024
            {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("bounded editor CLI deadline or output limit");
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        let output = Output {
            status,
            stdout: fs::read(stdout).unwrap(),
            stderr: fs::read(stderr).unwrap(),
        };
        assert!(output.stdout.len() + output.stderr.len() < 1024 * 1024);
        output
    }
    pub fn install(&self, code: &Path, user: &Path, extensions: &Path) -> PathBuf {
        let result = self.editor_cli(
            code,
            user,
            extensions,
            &["--install-extension", &native(&self.vsix), "--force"],
        );
        assert!(
            result.status.success(),
            "VSIX installation: {} {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        self.installed(extensions)
    }
    pub fn install_driver(
        &self,
        repo: &Path,
        code: &Path,
        user: &Path,
        extensions: &Path,
        source: &Path,
    ) {
        let archive = self.root.join("qualification-driver.vsix");
        let result = Command::new(std::env::var_os("VCP_TEST_NODE").expect("pinned Node required"))
            .arg(repo.join("src/packages/vscode/node_modules/@vscode/vsce/vsce"))
            .args([
                "package",
                "--no-dependencies",
                "--allow-missing-repository",
                "--skip-license",
                "--out",
            ])
            .arg(&archive)
            .current_dir(source)
            .creation_flags(HIDDEN)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "qualification driver packaging: {} {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        let installed = self.editor_cli(
            code,
            user,
            extensions,
            &["--install-extension", &native(&archive), "--force"],
        );
        assert!(
            installed.status.success(),
            "qualification driver installation: {} {}",
            String::from_utf8_lossy(&installed.stdout),
            String::from_utf8_lossy(&installed.stderr)
        );
    }
    pub fn installed(&self, extensions: &Path) -> PathBuf {
        fs::read_dir(extensions)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|path| {
                path.is_dir()
                    && path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("vcp.vcp-local-")
            })
            .expect("installed extension directory")
    }
    pub fn copy_driver(&self, repo: &Path, name: &str) -> PathBuf {
        let target = self.root.join(name);
        fs::copy(repo.join("src/packages/vscode/tests").join(name), &target).unwrap();
        target
    }
    pub fn launcher(&self, repo: &Path) -> PathBuf {
        let target = self.root.join("run-extension-host.ps1");
        fs::copy(
            repo.join("src/packages/vscode/scripts/run-extension-host.ps1"),
            &target,
        )
        .unwrap();
        target
    }
    pub fn launch(&self, script: &Path, input: &Path) -> Command {
        let mut command = Command::new(&self.pwsh);
        command
            .creation_flags(HIDDEN)
            .env("PATH", restricted_path())
            .env_remove("NODE_PATH")
            .env_remove("VSCODE_DEV")
            .current_dir(&self.root)
            .args(["-NoProfile", "-File"])
            .arg(script)
            .arg("-InputFile")
            .arg(input);
        command
    }
}
