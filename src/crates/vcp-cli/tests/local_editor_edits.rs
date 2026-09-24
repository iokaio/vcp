// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! P4-03 prerequisite: qualify the real editor's per-document version fence.
use serde_json::json;
use std::{fs, path::Path, path::PathBuf, process::Command};

fn native(path: &Path) -> String {
    path.to_str()
        .unwrap()
        .strip_prefix(r"\\?\")
        .unwrap_or(path.to_str().unwrap())
        .to_owned()
}

#[test]
#[ignore = "requires pinned VS Code 1.138, native normal user token"]
fn editor_version_bound_edits_preserve_typing_and_partial_results() {
    let code = PathBuf::from(std::env::var_os("VCP_TEST_CODE").expect("pinned editor required"));
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let output = repo.join("artifacts/p4-editor-edit-primitives");
    fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let user = profile.path().join("user-data");
    fs::create_dir_all(user.join("User")).unwrap();
    fs::write(user.join("User/settings.json"),serde_json::to_vec(&json!({"security.workspace.trust.enabled":true,"security.workspace.trust.startupPrompt":"never","security.workspace.trust.emptyWindow":false,"update.mode":"none","extensions.autoUpdate":false,"extensions.autoCheckUpdates":false,"telemetry.telemetryLevel":"off","workbench.startupEditor":"none","window.restoreWindows":"none","files.autoSave":"off","files.trimTrailingWhitespace":false,"files.insertFinalNewline":false,"editor.formatOnSave":false})).unwrap()).unwrap();
    let mut files = Vec::new();
    let mut roots = Vec::new();
    for name in ["first", "second"] {
        let root = profile.path().join(name);
        fs::create_dir(&root).unwrap();
        let file = root.join("same.txt");
        fs::write(
            &file,
            b"\xef\xbb\xbfbase \xce\xb1\xf0\x9f\x98\x80\r\nsecond\r\n",
        )
        .unwrap();
        files.push(file);
        roots.push(root);
    }
    let workspace = profile.path().join("qualification.code-workspace");
    fs::write(&workspace,serde_json::to_vec(&json!({"folders":roots.iter().map(|root|json!({"path":native(root)})).collect::<Vec<_>>()})).unwrap()).unwrap();
    let extensions = profile.path().join("extensions");
    fs::create_dir(&extensions).unwrap();
    let extension = profile.path().join("inert-extension");
    fs::create_dir(&extension).unwrap();
    fs::write(extension.join("package.json"),r#"{"name":"primitive-target","publisher":"vcp-test","version":"0.0.1","engines":{"vscode":"1.138.0"}}"#).unwrap();
    let driver = profile.path().join("qualification-driver");
    fs::create_dir(&driver).unwrap();
    fs::write(driver.join("package.json"),r#"{"name":"primitive-driver","publisher":"vcp-test","version":"0.0.1","engines":{"vscode":"1.138.0"},"activationEvents":["*"],"main":"./driver.cjs","extensionKind":["workspace"],"capabilities":{"untrustedWorkspaces":{"supported":true}}}"#).unwrap();
    fs::write(driver.join("driver.cjs"),r#"const vscode=require('vscode');const fs=require('node:fs');exports.activate=()=>{setImmediate(async()=>{try{const input=JSON.parse(fs.readFileSync(process.env.VCP_EXTENSION_TEST_INPUT,'utf8'));await require(input.runner).run();}catch{}finally{void vscode.commands.executeCommand('workbench.action.quit');}});};"#).unwrap();
    let result = profile.path().join("result.json");
    let input = profile.path().join("input.json");
    fs::write(&input,serde_json::to_vec(&json!({"code":native(&code),"extension":native(&extension),"driver":native(&driver),"runner":native(&repo.join("src/packages/vscode/tests/editor-edits-host.cjs")),"workspaceFile":native(&workspace),"userData":native(&user),"extensions":native(&extensions),"result":native(&result),"stdout":native(&output.join("stdout.log")),"stderr":native(&output.join("stderr.log")),"diagnostics":native(&output.join("editor-logs")),"runtimeEvidence":native(&output.join("runtime.json")),"files":files.iter().map(|file|native(file)).collect::<Vec<_>>()})).unwrap()).unwrap();
    let status = Command::new("pwsh")
        .args(["-NoProfile", "-File"])
        .arg(native(
            &repo.join("src/packages/vscode/scripts/run-extension-host.ps1"),
        ))
        .arg("-InputFile")
        .arg(native(&input))
        .status()
        .unwrap();
    if let Ok(bytes) = fs::read(&result) {
        fs::write(output.join("result.json"), bytes).unwrap();
    }
    assert!(
        status.success(),
        "inspect artifacts/p4-editor-edit-primitives"
    );
    let evidence: serde_json::Value = serde_json::from_slice(&fs::read(result).unwrap()).unwrap();
    assert_eq!(evidence["ok"], true, "{evidence}");
    for field in ["typingRace", "partial", "undoRedo", "bomCrlf"] {
        assert_eq!(evidence[field], true);
    }
    assert_eq!(
        fs::read(&files[0]).unwrap(),
        b"\xef\xbb\xbfprepared \xce\xb1\xf0\x9f\x98\x80\r\nsecond\r\n"
    );
    assert_eq!(
        fs::read(&files[1]).unwrap(),
        b"\xef\xbb\xbfbase \xce\xb1\xf0\x9f\x98\x80\r\nsecond\r\n"
    );
}
