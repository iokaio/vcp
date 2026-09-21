// SPDX-License-Identifier: Apache-2.0
//! Explicit host profiles prepare opaque native process operations. A profile
//! or serialized operation is not permission to execute it.
pub mod output;
use crate::*;
use std::{collections::BTreeMap, path::PathBuf};
use vcp_repository::{path::HeldPath, FileVersion, RootIdentity};

/// Foreground processes share the coding task's finite one-hour host bound.
pub const MAX_TIMEOUT_MS: u64 = 3_600_000;
pub const DEFAULT_TIMEOUT_MS: u64 = 120_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Direct,
    PowerShell,
    Cmd,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Terminal {
    pub rows: u16,
    pub cols: u16,
}
/// Explicit reduced-isolation selection. Restricted requirements are checked
/// separately against actual platform capabilities and cannot be approved away.
#[derive(Clone, Debug, Serialize)]
pub struct Profile {
    name: String,
    executable: PathBuf,
    mode: Mode,
    environment: BTreeMap<String, String>,
    required: BTreeSet<Isolation>,
    reduced_isolation: bool,
    inputs: Vec<String>,
    terminal: Option<Terminal>,
    process_count: u32,
    max_timeout_ms: u64,
    output_encoding: Option<output::Encoding>,
}
impl Profile {
    pub fn new(
        name: String,
        executable: PathBuf,
        mode: Mode,
        environment: BTreeMap<String, String>,
        required: BTreeSet<Isolation>,
        reduced_isolation: bool,
    ) -> Result<Self> {
        if name.is_empty()
            || name.len() > 64
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || !executable.is_absolute()
            || executable
                .extension()
                .is_none_or(|s| !s.eq_ignore_ascii_case("exe"))
        {
            return Err(Error::Invalid("explicit named executable profile required"));
        }
        // Never inherit ambient credentials. Only these non-secret bootstrap
        // settings are accepted; credentials must not be put in their values.
        let allowed = [
            "SYSTEMROOT",
            "WINDIR",
            "PATH",
            "PATHEXT",
            "TEMP",
            "TMP",
            "LANG",
            "LC_ALL",
            "TERM",
            "CI",
            "RUST_BACKTRACE",
            "CARGO_TARGET_DIR",
            "CARGO_HOME",
            "LIB",
            "INCLUDE",
            "LIBPATH",
        ];
        let mut names = BTreeSet::new();
        for (key, value) in &environment {
            let normalized = key.to_ascii_uppercase();
            if !allowed.contains(&normalized.as_str())
                || !names.insert(normalized)
                || value.contains('\0')
                || value.len() > 32 * 1024
            {
                return Err(Error::Invalid(
                    "unsupported or duplicate public environment setting",
                ));
            }
        }
        if environment.values().map(String::len).sum::<usize>() > 64 * 1024 {
            return Err(Error::Invalid("environment ceiling"));
        }
        Ok(Self {
            name,
            executable,
            mode,
            environment,
            required,
            reduced_isolation,
            inputs: vec![],
            terminal: None,
            process_count: 32,
            max_timeout_ms: DEFAULT_TIMEOUT_MS,
            output_encoding: None,
        })
    }
    /// Trusted read-only script/config dependencies stay pinned for the entire
    /// process lifetime. They cannot be supplied or removed by model arguments.
    pub fn with_inputs(mut self, inputs: Vec<String>) -> Result<Self> {
        if inputs.len() > 256 {
            return Err(Error::Invalid("profile input ceiling"));
        }
        let mut names = BTreeSet::new();
        for path in &inputs {
            checked_path(path, false)?;
            if !names.insert(path.to_lowercase()) {
                return Err(Error::Invalid("overlapping profile inputs"));
            }
        }
        self.inputs = inputs;
        Ok(self)
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Host ceiling, including the root. Model arguments cannot raise this limit.
    pub fn with_process_count(mut self, count: u32) -> Result<Self> {
        if !(1..=128).contains(&count) {
            return Err(Error::Invalid("process count ceiling"));
        }
        self.process_count = count;
        Ok(self)
    }
    /// Trusted profile ceiling. Serialized model requests cannot change it.
    pub fn with_max_timeout_ms(mut self, maximum: u64) -> Result<Self> {
        if !(1..=MAX_TIMEOUT_MS).contains(&maximum) {
            return Err(Error::Invalid(
                "profile duration exceeds finite host ceiling",
            ));
        }
        self.max_timeout_ms = maximum;
        Ok(self)
    }
    pub fn with_output_encoding(mut self, encoding: Option<output::Encoding>) -> Result<Self> {
        if self.terminal.is_some() && encoding == Some(output::Encoding::Utf16Le) {
            return Err(Error::Invalid("UTF-16LE output requires a pipe profile"));
        }
        self.output_encoding = encoding;
        Ok(self)
    }
    pub fn output_encoding(&self) -> Option<output::Encoding> {
        self.output_encoding
    }
    pub fn max_timeout_ms(&self) -> u64 {
        self.max_timeout_ms
    }
    pub fn process_count(&self) -> u32 {
        self.process_count
    }
    pub fn with_terminal(mut self, rows: u16, cols: u16) -> Result<Self> {
        if !(1..=500).contains(&rows)
            || !(1..=500).contains(&cols)
            || self.mode == Mode::Cmd
            || self.output_encoding == Some(output::Encoding::Utf16Le)
        {
            return Err(Error::Invalid(
                "terminal dimensions or unsupported cmd PTY conversion",
            ));
        }
        self.terminal = Some(Terminal { rows, cols });
        Ok(self)
    }
    pub fn terminal(&self) -> Option<Terminal> {
        self.terminal
    }
    pub fn executable_root_id(&self) -> Result<RootId> {
        RootId::parse(format!("exec-{}", self.name)).map_err(|_| Error::Invalid("profile identity"))
    }
    pub fn mode(&self) -> Mode {
        self.mode
    }
    pub fn digest(&self) -> Result<String> {
        Ok(vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(
            self,
        )?))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub profile: String,
    pub arguments: Vec<String>,
    pub directory: String,
    pub timeout_ms: u64,
    pub output_bytes: u64,
    #[serde(default)]
    pub input: Option<String>,
}
pub fn definition() -> serde_json::Value {
    serde_json::json!({"type":"function","name":"vcp_exec","strict":true,"description":"Run an explicit executable profile. Shell profiles take one script. Terminal profiles accept bounded initial input and produce merged terminal output. Processes are opaque effects requiring current authority.","parameters":{"type":"object","properties":{"profile":{"type":"string"},"arguments":{"type":"array","items":{"type":"string"}},"directory":{"type":"string"},"timeout_ms":{"type":"integer"},"output_bytes":{"type":"integer"},"input":{"type":["string","null"]}},"required":["profile","arguments","directory","timeout_ms","output_bytes","input"],"additionalProperties":false}})
}
impl Request {
    pub fn from_arguments(arguments: &str) -> Result<Self> {
        if arguments.len() > 128 * 1024 {
            return Err(Error::Invalid("process argument ceiling"));
        }
        Ok(serde_json::from_str(arguments)?)
    }
}
pub struct Prepared {
    authority: vcp_policy::Prepared,
    root: Root,
    executable_root: Root,
    executable: FileVersion,
    directory: String,
    directory_identity: String,
    profile: Profile,
    arguments: Vec<String>,
    snapshot: serde_json::Value,
    inputs: Vec<FileVersion>,
    input: Option<String>,
}
pub struct Pins {
    pub directory: HeldPath,
    pub executable: HeldPath,
    pub inputs: Vec<HeldPath>,
}
impl Prepared {
    pub fn input(&self) -> Option<&str> {
        self.input.as_deref()
    }
    pub fn authority(&self) -> &vcp_policy::Prepared {
        &self.authority
    }
    pub fn root(&self) -> &Root {
        &self.root
    }
    pub fn profile(&self) -> &Profile {
        &self.profile
    }
    pub fn executable(&self) -> PathBuf {
        self.executable_root.path().join(&self.executable.path)
    }
    pub fn directory(&self) -> PathBuf {
        self.root.path().join(&self.directory)
    }
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }
    pub fn environment(&self) -> &BTreeMap<String, String> {
        &self.profile.environment
    }
    pub fn roots(&self) -> BTreeSet<RootId> {
        BTreeSet::from([
            self.root.identity.root.clone(),
            self.executable_root.identity.root.clone(),
        ])
    }
    pub fn pin(&self) -> Result<Pins> {
        let directory = self.root.hold(
            if self.directory.is_empty() {
                None
            } else {
                Some(Path::new(&self.directory))
            },
            true,
        )?;
        if directory.native_identity != self.directory_identity {
            return Err(vcp_repository::Error::Stale.into());
        }
        let executable = self.executable_root.pin_version(&self.executable)?;
        let inputs = self
            .inputs
            .iter()
            .map(|source| self.root.pin_version(source))
            .collect::<vcp_repository::Result<Vec<_>>>()?;
        if snapshot(&self.root)? != self.snapshot {
            return Err(vcp_repository::Error::Stale.into());
        }
        Ok(Pins {
            directory,
            executable,
            inputs,
        })
    }
    pub fn evidence(&self) -> Result<Vec<u8>> {
        Ok(vcp_protocol::canonical_bytes(
            &serde_json::json!({"schema_version":1,"operation":self.authority.operation(),"profile":self.profile,"executable":self.executable,"directory_identity":self.directory_identity,"sources":self.snapshot,"pinned_inputs":self.inputs,"reduced_isolation":self.profile.reduced_isolation}),
        )?)
    }
}
fn snapshot(root: &Root) -> Result<serde_json::Value> {
    let discovery = root.discover(&vcp_repository::discovery::Limits::default())?;
    if !discovery.complete {
        return Err(Error::Invalid(
            "process source snapshot exceeds bounded scope",
        ));
    }
    Ok(
        serde_json::json!({"sources":discovery.sources.iter().map(|s|&s.version).collect::<Vec<_>>(),"exclusions":discovery.exclusions,"ignore_dependencies":discovery.ignore_dependencies}),
    )
}
pub fn prepare(
    root: Root,
    identity: Identity,
    profile: Profile,
    request: Request,
) -> Result<Prepared> {
    let executable_root_id = profile.executable_root_id()?;
    if executable_root_id == root.identity.root {
        return Err(Error::Invalid(
            "executable and workspace root identities collide",
        ));
    }
    checked_path(&request.directory, true)?;
    if request.timeout_ms == 0 || request.timeout_ms > profile.max_timeout_ms {
        return Err(Error::Invalid(
            "requested process duration exceeds trusted profile ceiling",
        ));
    }
    if request.profile != profile.name
        || root.identity.workspace != identity.scope.workspace
        || root.identity.binding != identity.binding
        || request.arguments.len() > 256
        || request.arguments.iter().any(|s| s.contains('\0'))
        || request.arguments.iter().map(String::len).sum::<usize>() > 24 * 1024
        || request.output_bytes == 0
        || request.output_bytes > 8 * 1024 * 1024
        || request
            .input
            .as_ref()
            .is_some_and(|s| s.len() > 32 * 1024 || s.contains('\0') || profile.terminal.is_none())
    {
        return Err(Error::Invalid("process scope or bounds"));
    }
    let parent = profile
        .executable
        .parent()
        .ok_or(Error::Invalid("executable parent"))?;
    let name = profile
        .executable
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or(Error::Invalid("Unicode executable name"))?;
    let executable_root = Root::open(
        RootIdentity {
            workspace: identity.scope.workspace.clone(),
            root: executable_root_id,
            repository: format!("host-profile:{}", profile.name),
            worktree: "executable".into(),
            binding: identity.binding,
        },
        parent,
    )?;
    let executable = executable_root.version(Path::new(name), 256 * 1024 * 1024)?;
    let directory = root.hold(
        if request.directory.is_empty() {
            None
        } else {
            Some(Path::new(&request.directory))
        },
        true,
    )?;
    let snapshot = snapshot(&root)?;
    let inputs = profile
        .inputs
        .iter()
        .map(|path| {
            root.read(Path::new(path), 1024 * 1024)
                .map(|source| source.version)
        })
        .collect::<vcp_repository::Result<Vec<_>>>()?;
    let mut arguments = match profile.mode {
        Mode::Direct => vec![],
        Mode::PowerShell => vec![
            "-NoLogo".into(),
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-Command".into(),
        ],
        Mode::Cmd => vec!["/d".into(), "/s".into(), "/c".into()],
    };
    if profile.mode != Mode::Direct && request.arguments.len() != 1 {
        return Err(Error::Invalid("shell profile requires one explicit script"));
    }
    arguments.extend(request.arguments.clone());
    let mut required = BTreeSet::from([
        Isolation::JobTree,
        Isolation::ProcessCount,
        Isolation::FilteredEnvironment,
        Isolation::Timeout,
        Isolation::OutputLimit,
    ]);
    required.extend(profile.required.iter().copied());
    if profile.terminal.is_some() {
        required.insert(Isolation::Pty);
    }
    if !profile.reduced_isolation {
        required.extend([Isolation::WorkspaceFilesystem, Isolation::NoNetwork]);
    }
    let operation = Operation {
        scope: identity.scope,
        actor: identity.actor,
        host: identity.host,
        binding: identity.binding,
        authority: identity.authority,
        steering: identity.steering,
        policy: identity.policy,
        tool: "vcp_exec".into(),
        schema: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&definition())?),
        arguments: String::from_utf8(vcp_protocol::canonical_bytes(
            &serde_json::json!({"request":request,"profile_digest":profile.digest()?}),
        )?)
        .unwrap(),
        invocation: Invocation::Process {
            executable: executable_root
                .path()
                .join(name)
                .to_str()
                .ok_or(Error::Invalid("Unicode executable path"))?
                .into(),
            executable_identity: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(
                &executable,
            )?),
            arguments: arguments.clone(),
            directory: root
                .path()
                .join(&request.directory)
                .to_str()
                .ok_or(Error::Invalid("Unicode directory"))?
                .into(),
            environment_digest: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(
                &profile.environment,
            )?),
            shell: profile.mode != Mode::Direct,
        },
        resources: vec![
            Resource {
                root: root.identity.root.clone(),
                path: String::new(),
                write: true,
                version: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(
                    &serde_json::json!({"directory":directory.native_identity,"sources":snapshot,"pinned_inputs":inputs}),
                )?),
            },
            Resource {
                root: executable_root.identity.root.clone(),
                path: executable.path.clone(),
                write: false,
                version: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&executable)?),
            },
        ],
        effects: BTreeSet::from([
            EffectClass::Read,
            EffectClass::Write,
            EffectClass::Execute,
            EffectClass::Network,
            EffectClass::Install,
            EffectClass::Publish,
            EffectClass::Opaque,
        ]),
        required_isolation: required,
        timeout_ms: Units::new(request.timeout_ms),
        output_bytes: ByteCount::new(request.output_bytes),
    };
    Ok(Prepared {
        authority: vcp_policy::Prepared::new(operation)?,
        root,
        executable_root,
        executable,
        directory: request.directory,
        directory_identity: directory.native_identity,
        profile,
        arguments,
        snapshot,
        inputs,
        input: request.input,
    })
}
