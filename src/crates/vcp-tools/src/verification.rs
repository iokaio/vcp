// SPDX-License-Identifier: Apache-2.0
//! Check discovery is data, never permission to execute or proof of success.
use crate::*;
use serde_json::Value;
use std::collections::BTreeMap;
use vcp_domain::verification::CheckOutcome;
use vcp_repository::{observation::Observation, FileVersion};
mod java_python;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Runner {
    Node,
    Cargo,
    Dotnet,
    Maven,
    Pytest,
}
/// Owner-selected Apache Maven installation, launched directly through Java.
/// These paths are configuration, not instructions discovered in project files.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MavenLauncher {
    pub classworlds_jar: String,
    pub classworlds_conf: String,
    pub home: String,
}
impl MavenLauncher {
    fn validate(&self) -> Result<()> {
        for path in [&self.classworlds_jar, &self.classworlds_conf, &self.home] {
            if path.len() > 4096
                || !Path::new(path).is_absolute()
                || path.chars().any(char::is_control)
                || path.contains(';')
                || Path::new(path)
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
            {
                return Err(Error::Invalid(
                    "literal absolute Maven installation paths required",
                ));
            }
        }
        let home = Path::new(&self.home);
        if !Path::new(&self.classworlds_jar).starts_with(home.join("boot"))
            || Path::new(&self.classworlds_jar)
                .extension()
                .is_none_or(|extension| extension != "jar")
            || Path::new(&self.classworlds_conf) != home.join("bin").join("m2.conf")
        {
            return Err(Error::Invalid(
                "Maven launcher must belong to its selected installation",
            ));
        }
        Ok(())
    }
}
/// Explicit owner acceptance, not model-supplied assertions about coverage.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub manifest: String,
    pub runner: Runner,
    pub profile: String,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maven: Option<MavenLauncher>,
    pub expected_tests: Vec<String>,
    pub rationale: String,
}
impl Requirement {
    pub fn validate(&self) -> Result<()> {
        match (&self.maven, self.runner) {
            (Some(launcher), Runner::Maven) => launcher.validate()?,
            (None, Runner::Maven) | (Some(_), _) => {
                return Err(Error::Invalid(
                    "Maven launcher configuration is required only for Maven checks",
                ));
            }
            (None, _) => {}
        }
        if vcp_repository::path::relative(Path::new(&self.manifest))? != self.manifest {
            return Err(Error::Invalid("normalized manifest path required"));
        }
        if self
            .timeout_ms
            .is_some_and(|duration| !(1..=process::MAX_TIMEOUT_MS).contains(&duration))
        {
            return Err(Error::Invalid(
                "verification check duration exceeds finite host ceiling",
            ));
        }
        if self.profile.is_empty()
            || self.profile.len() > 64
            || self.expected_tests.is_empty()
            || self.expected_tests.len() > 256
            || self
                .expected_tests
                .iter()
                .any(|s| s.trim().is_empty() || s.len() > 1024 || s.contains(['\r', '\n', '\0']))
            || self.expected_tests.iter().collect::<BTreeSet<_>>().len()
                != self.expected_tests.len()
            || self.rationale.trim().is_empty()
            || self.rationale.len() > 4096
        {
            return Err(Error::Invalid("bounded verification acceptance required"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub specification: String,
    pub origin: Option<FileVersion>,
    pub directory: String,
    pub runner: Runner,
    pub request: process::Request,
    pub expected_tests: Vec<String>,
    pub rationale: String,
    pub not_run: Option<String>,
}

/// Owner-selected diagnostic checks. Missing or ambiguous project coverage
/// deliberately falls back to the complete configured requirement set.
pub fn focused_requirements(
    requirements: &[Requirement],
    affected_paths: &[String],
    failed_checks: &[String],
) -> Result<Vec<Requirement>> {
    Ok(select_focused_requirements(requirements, affected_paths, failed_checks)?.requirements)
}

pub struct FocusedRequirements {
    pub requirements: Vec<Requirement>,
    pub full_fallback: bool,
}

pub fn select_focused_requirements(
    requirements: &[Requirement],
    affected_paths: &[String],
    failed_checks: &[String],
) -> Result<FocusedRequirements> {
    let fallback = || FocusedRequirements {
        requirements: requirements.to_vec(),
        full_fallback: true,
    };
    if affected_paths.len() > 256 || failed_checks.len() > 32 {
        return Err(Error::Invalid("focused verification selector ceiling"));
    }
    if affected_paths.is_empty() && failed_checks.is_empty() {
        return Ok(fallback());
    }
    let mut selected = BTreeSet::new();
    for path in affected_paths {
        let normalized = vcp_repository::path::relative(Path::new(path))?;
        if normalized != *path {
            return Err(Error::Invalid("normalized affected path required"));
        }
        let matches: Vec<_> = requirements
            .iter()
            .enumerate()
            .filter(|(_, requirement)| {
                let directory = requirement
                    .manifest
                    .rsplit_once('/')
                    .map_or("", |(directory, _)| directory);
                directory.is_empty() || path.starts_with(&format!("{directory}/"))
            })
            .map(|(index, _)| index)
            .collect();
        if matches.len() != 1 {
            return Ok(fallback());
        }
        selected.insert(matches[0]);
    }
    for specification in failed_checks {
        let matches: Vec<_> = requirements
            .iter()
            .enumerate()
            .filter(|(_, requirement)| format!("{}#test", requirement.manifest) == *specification)
            .map(|(index, _)| index)
            .collect();
        if matches.len() != 1 {
            return Ok(fallback());
        }
        selected.insert(matches[0]);
    }
    Ok(FocusedRequirements {
        requirements: requirements
            .iter()
            .enumerate()
            .filter(|(index, _)| selected.contains(index))
            .map(|(_, requirement)| requirement.clone())
            .collect(),
        full_fallback: false,
    })
}

#[cfg(test)]
mod focused_tests {
    use super::*;
    fn requirement(manifest: &str) -> Requirement {
        Requirement {
            manifest: manifest.into(),
            runner: Runner::Node,
            maven: None,
            profile: "node".into(),
            timeout_ms: None,
            expected_tests: vec!["acceptance".into()],
            rationale: "owner check".into(),
        }
    }
    #[test]
    fn exact_projects_and_failed_checks_form_a_stable_union() {
        let requirements = vec![
            requirement("api/package.json"),
            requirement("ui/package.json"),
        ];
        let selected = focused_requirements(&requirements, &["ui/src/app.ts".into()], &[]).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].manifest, "ui/package.json");
        assert!(
            !select_focused_requirements(&requirements, &["ui/src/app.ts".into()], &[])
                .unwrap()
                .full_fallback
        );
        let selected = focused_requirements(
            &requirements,
            &["ui/src/app.ts".into()],
            &["api/package.json#test".into()],
        )
        .unwrap();
        assert_eq!(
            selected
                .iter()
                .map(|r| r.manifest.as_str())
                .collect::<Vec<_>>(),
            vec!["api/package.json", "ui/package.json"]
        );
        assert!(
            !select_focused_requirements(
                &requirements,
                &["ui/src/app.ts".into()],
                &["api/package.json#test".into()]
            )
            .unwrap()
            .full_fallback,
            "selecting every requirement explicitly is not a fallback"
        );
    }
    #[test]
    fn unknown_or_ambiguous_coverage_falls_back_and_invalid_paths_reject() {
        let requirements = vec![
            requirement("api/package.json"),
            requirement("ui/package.json"),
        ];
        for (paths, failed) in [
            (vec![], vec![]),
            (vec!["shared/types.ts".into()], vec![]),
            (vec![], vec!["unknown#test".into()]),
        ] {
            assert!(
                select_focused_requirements(&requirements, &paths, &failed)
                    .unwrap()
                    .full_fallback
            );
            assert_eq!(
                focused_requirements(&requirements, &paths, &failed)
                    .unwrap()
                    .len(),
                2
            );
        }
        let overlap = vec![requirement("package.json"), requirement("ui/package.json")];
        assert_eq!(
            focused_requirements(&overlap, &["ui/app.ts".into()], &[])
                .unwrap()
                .len(),
            2
        );
        assert!(focused_requirements(&requirements, &["../private".into()], &[]).is_err());
    }
}
/// Discover only qualified command forms. Shell syntax and pre/post hooks are
/// not silently discarded. Unsupported configurations remain visible as not run.
pub fn discover(observation: &Observation, requirements: &[Requirement]) -> Result<Vec<Plan>> {
    if requirements.len() > 32 {
        return Err(Error::Invalid("verification plan count"));
    }
    let mut seen = BTreeSet::new();
    requirements.iter().map(|requirement| {
        requirement.validate()?;
        if !seen.insert(requirement.manifest.to_lowercase()) {
            return Err(Error::Invalid("duplicate verification manifest"));
        }
        let directory = requirement.manifest.rsplit_once('/').map_or("", |(dir, _)| dir).to_owned();
        let source = observation.sources.iter().find(|s| s.version.path == requirement.manifest);
        let mut plan = Plan {
            specification: format!("{}#test", requirement.manifest),
            origin: source.map(|s| s.version.clone()),
            directory: directory.clone(), runner: requirement.runner,
            request: process::Request { profile: requirement.profile.clone(), arguments: vec![], directory,
                timeout_ms: requirement.timeout_ms.unwrap_or(process::DEFAULT_TIMEOUT_MS), output_bytes: 1024 * 1024, input: None },
            expected_tests: requirement.expected_tests.clone(), rationale: requirement.rationale.clone(), not_run: None,
        };
        let discovered: std::result::Result<Vec<String>, String> = (|| {
            if !observation.manifest.bounded_scan_complete { return Err("workspace scan is incomplete".into()); }
            let source = source.ok_or("required project manifest is unavailable or excluded")?;
            match requirement.runner {
                Runner::Node => {
                    if requirement.manifest.rsplit('/').next() != Some("package.json") { return Err("Node check requires package.json".into()); }
                    let value: Value = serde_json::from_slice(&source.bytes).map_err(|_| "invalid package.json")?;
                    let scripts = value.get("scripts").and_then(Value::as_object).ok_or("package.json has no scripts")?;
                    if scripts.contains_key("pretest") || scripts.contains_key("posttest") { return Err("npm pretest/posttest hooks require a separate qualified plan".into()); }
                    let script = scripts.get("test").and_then(Value::as_str).ok_or("package.json has no test script")?;
                    let words: Vec<_> = script.split_ascii_whitespace().collect();
                    if words.len() < 3 || words[..2] != ["node", "--test"] || words.len() > 66 {
                        return Err("supported test script is node --test followed by explicit relative test files".into());
                    }
                    let mut targets = BTreeSet::new();
                    for target in &words[2..] {
                        if !target.bytes().all(|b| b.is_ascii_alphanumeric() || b"/_-.".contains(&b))
                            || target.starts_with('-') || !targets.insert(target.to_lowercase()) {
                            return Err("test targets must be distinct literal relative paths".into());
                        }
                        vcp_repository::path::relative(Path::new(target)).map_err(|_| "test target escapes project directory")?;
                        let path = if plan.directory.is_empty() { target.to_string() } else { format!("{}/{target}", plan.directory) };
                        if !observation.sources.iter().any(|s| s.version.path == path) {
                            return Err(format!("test target is unavailable or excluded: {path}"));
                        }
                    }
                    Ok([vec!["--test".into(), "--test-reporter=tap".into(), "--test-concurrency=1".into()], words[2..].iter().map(|s| s.to_string()).collect()].concat())
                }
                Runner::Cargo => {
                    if requirement.manifest.rsplit('/').next() != Some("Cargo.toml") { return Err("Cargo check requires Cargo.toml".into()); }
                    // Cargo, not a second partial TOML parser, validates the manifest.
                    Ok(["test", "--locked", "--offline", "--manifest-path", "Cargo.toml", "--all-targets", "--", "--test-threads=1"].into_iter().map(str::to_owned).collect())
                }
                Runner::Dotnet => {
                    let filename = requirement.manifest.rsplit('/').next().ok_or("missing .NET project filename")?;
                    dotnet_projects(filename, &source.bytes)?.into_iter().try_for_each(|project| {
                        let path = if plan.directory.is_empty() { project.clone() } else { format!("{}/{project}", plan.directory) };
                        if observation.sources.iter().any(|s| s.version.path == path) { Ok(()) }
                        else { Err(format!("solution project is unavailable or excluded: {path}")) }
                    })?;
                    // MSBuild validates project XML. The owner selects the direct
                    // executable profile; discovery never enables a shell or restore.
                    Ok(["test", filename, "--no-restore", "--nologo", "--logger", "console;verbosity=normal", "--disable-build-servers"].into_iter().map(str::to_owned).collect())
                }
                Runner::Maven => {
                    if requirement.manifest.rsplit('/').next() != Some("pom.xml") { return Err("Maven check requires pom.xml".into()); }
                    let launcher = requirement.maven.as_ref().ok_or("Maven Java launcher is not configured")?;
                    // Literal JVM arguments avoid mvn.cmd/shell interpretation. The
                    // current observed project is selected by the process directory.
                    let mut arguments = vec!["-classpath".into(), launcher.classworlds_jar.clone(),
                        format!("-Dclassworlds.conf={}", launcher.classworlds_conf), format!("-Dmaven.home={}", launcher.home),
                        "-Dmaven.multiModuleProjectDirectory=.".into(), "org.codehaus.plexus.classworlds.launcher.Launcher".into()];
                    arguments.extend(["-B", "-ntp", "-o", "-Dstyle.color=never", "-Dsurefire.useFile=false", "-Dsurefire.reportFormat=plain",
                        "-DskipTests=false", "-Dmaven.test.skip=false", "-DfailIfNoTests=true", "clean", "test"].into_iter().map(str::to_owned));
                    Ok(arguments)
                }
                Runner::Pytest => {
                    if requirement.manifest.rsplit('/').next() != Some("pyproject.toml") { return Err("pytest check requires pyproject.toml".into()); }
                    // The selected Python installation supplies pytest. Clear
                    // configured filtering/quiet options and require named results.
                    // -B alone still reads timestamp-based .pyc files. A namespace
                    // derived from every observed source identity prevents an
                    // ordinary stale cache from hiding same-size/same-time edits.
                    let source_identity = vcp_protocol::canonical_bytes(&observation.manifest)
                        .map_err(|_| "cannot fingerprint Python verification inputs")?;
                    let digest = vcp_protocol::digest_bytes(&source_identity);
                    let mut arguments = vec!["-B".into(), "-X".into(), format!("pycache_prefix=.vcp-verification-pycache/{digest}")];
                    arguments.extend(["-m", "pytest", "-vv", "--color=no", "-o", "addopts=", "-p", "no:cacheprovider"].into_iter().map(str::to_owned));
                    Ok(arguments)
                }
            }
        })();
        match discovered { Ok(arguments) => plan.request.arguments = arguments, Err(reason) => plan.not_run = Some(reason) }
        Ok(plan)
    }).collect()
}

fn dotnet_projects(filename: &str, bytes: &[u8]) -> std::result::Result<Vec<String>, String> {
    if filename.starts_with('-') {
        return Err(".NET manifest cannot be a command option".into());
    }
    if bytes.len() > 1024 * 1024 {
        return Err("solution exceeds parser bound".into());
    }
    if filename.ends_with(".csproj") {
        return Ok(vec![filename.into()]);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "solution is not UTF-8")?;
    let mut projects = BTreeSet::new();
    let mut add = |path: String| -> std::result::Result<(), String> {
        let path = path.replace('\\', "/");
        if !path.ends_with(".csproj")
            || vcp_repository::path::relative(Path::new(&path))
                .ok()
                .as_deref()
                != Some(&path)
            || path.starts_with('-')
            || path.contains(['\r', '\n', '\0'])
            || !projects.insert(path.to_lowercase())
        {
            return Err("solution projects must be distinct literal relative .csproj paths".into());
        }
        if projects.len() > 256 {
            return Err("solution project count exceeds parser bound".into());
        }
        Ok(())
    };
    // Keep original casing for exact source identity lookup on every platform.
    let mut paths = Vec::new();
    if filename.ends_with(".sln") {
        let project = regex::Regex::new(
            r#"^Project\("\{[0-9A-Fa-f-]{36}\}"\) = "[^"]+", "([^"]+)", "\{[0-9A-Fa-f-]{36}\}"$"#,
        )
        .map_err(|_| "invalid solution parser")?;
        if !text
            .trim_start_matches('\u{feff}')
            .trim_start()
            .starts_with("Microsoft Visual Studio Solution File, Format Version ")
        {
            return Err("unsupported .sln header".into());
        }
        for line in text.lines().map(str::trim) {
            if line.starts_with("Project(") {
                let captures = project
                    .captures(line)
                    .ok_or("malformed solution project record")?;
                // Solution folders have no project file and cannot provide test coverage.
                if line.starts_with("Project(\"{2150E333-8FDC-42A3-9474-1A3956D46DE8}\")") {
                    continue;
                }
                let path = captures[1].replace('\\', "/");
                add(path.clone())?;
                paths.push(path);
            }
        }
    } else if filename.ends_with(".slnx") {
        use quick_xml::events::Event;
        let mut reader = quick_xml::Reader::from_str(text.trim_start_matches('\u{feff}'));
        reader.config_mut().trim_text(true);
        let mut stack: Vec<Vec<u8>> = Vec::new();
        let mut root_seen = false;
        loop {
            let event = reader.read_event().map_err(|_| "malformed .slnx XML")?;
            let empty = matches!(&event, Event::Empty(_));
            match event {
                Event::Start(element) | Event::Empty(element) => {
                    let name = element.name().as_ref().to_vec();
                    let mut attrs = BTreeMap::new();
                    for attribute in element.attributes() {
                        let attribute = attribute.map_err(|_| "malformed solution attribute")?;
                        let value = std::str::from_utf8(attribute.value.as_ref())
                            .map_err(|_| "solution attribute is not UTF-8")?;
                        if value.contains('&')
                            || attrs
                                .insert(attribute.key.as_ref().to_vec(), value.to_owned())
                                .is_some()
                        {
                            return Err(
                                "solution entities or duplicate attributes are unsupported".into(),
                            );
                        }
                    }
                    match name.as_slice() {
                        b"Solution" if stack.is_empty() && !root_seen && attrs.is_empty() => {
                            root_seen = true
                        }
                        b"Folder"
                            if stack
                                .last()
                                .is_some_and(|n| n == b"Solution" || n == b"Folder")
                                && attrs.len() == 1
                                && attrs.contains_key(b"Name".as_slice()) => {}
                        b"Project"
                            if stack
                                .last()
                                .is_some_and(|n| n == b"Solution" || n == b"Folder")
                                && empty
                                && attrs.len() == 1
                                && attrs.contains_key(b"Path".as_slice()) =>
                        {
                            let path = attrs
                                .remove(b"Path".as_slice())
                                .ok_or("missing project path")?
                                .replace('\\', "/");
                            add(path.clone())?;
                            paths.push(path);
                        }
                        _ => return Err("unsupported .slnx element or attributes".into()),
                    }
                    if !empty {
                        if stack.len() >= 32 {
                            return Err("solution XML depth exceeds parser bound".into());
                        }
                        stack.push(name);
                    }
                }
                Event::End(element) => {
                    if stack.pop().as_deref() != Some(element.name().as_ref()) {
                        return Err("malformed solution nesting".into());
                    }
                }
                Event::Text(value) if value.iter().all(u8::is_ascii_whitespace) => {}
                Event::Decl(_) | Event::Comment(_) => {}
                Event::Eof => break,
                _ => return Err("solution DTD, entities and content are unsupported".into()),
            }
        }
        if !root_seen || !stack.is_empty() {
            return Err("incomplete .slnx document".into());
        }
    } else {
        return Err(".NET check requires .csproj, .sln or .slnx".into());
    }
    if paths.is_empty() {
        return Err("solution contains no qualified projects".into());
    }
    Ok(paths)
}

/// Runner output is an observation from untrusted project execution. It is
/// useful only together with the exact native process and input receipts.
pub fn evaluate(
    plan: &Plan,
    exit_code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
    reason: Option<&str>,
) -> CheckOutcome {
    let fail = |reason: &str| CheckOutcome::Failed {
        reason: reason.into(),
    };
    if let Some(reason) = &plan.not_run {
        return CheckOutcome::NotRun {
            reason: reason.clone(),
        };
    }
    if let Some(reason) = reason {
        return fail(reason);
    }
    if exit_code != Some(0) {
        return fail("check did not exit successfully");
    }
    if stdout.len().saturating_add(stderr.len()) > 1024 * 1024 {
        return fail("check output exceeds parser bound");
    }
    let Ok(stdout) = std::str::from_utf8(stdout) else {
        return fail("check stdout is not UTF-8");
    };
    let Ok(stderr) = std::str::from_utf8(stderr) else {
        return fail("check stderr is not UTF-8");
    };
    let mut passed = BTreeSet::new();
    match plan.runner {
        Runner::Maven | Runner::Pytest => {
            passed = match java_python::results(plan.runner, stdout, stderr) {
                Ok(results) => results,
                Err(reason) => return fail(reason),
            };
        }
        Runner::Dotnet => {
            let Ok(duration_pattern) = regex::Regex::new(
                r"^(?:< )?[0-9]+(?:\.[0-9]+)? (?:ms|s|m|h)(?: [0-9]+(?:\.[0-9]+)? (?:ms|s|m|h))*\]$",
            ) else {
                return fail("invalid .NET duration parser");
            };
            let mut total = None;
            let mut summary_passed = None;
            let mut successful = false;
            let mut other_counts = BTreeMap::new();
            for line in stdout.lines().chain(stderr.lines()) {
                let row = line.trim();
                if row == "Test Run Successful." {
                    if successful {
                        return fail("duplicate .NET success summary");
                    }
                    successful = true;
                }
                if row == "Test Run Failed."
                    || row == "Test Run Aborted."
                    || ["Failed ", "Skipped "].iter().any(|prefix| {
                        row.strip_prefix(*prefix)
                            .and_then(|result| result.rsplit_once(" ["))
                            .is_some_and(|(name, duration)| {
                                !name.is_empty() && duration_pattern.is_match(duration)
                            })
                    })
                {
                    return fail(".NET runner reports failed or skipped tests");
                }
                if let Some(result) = row.strip_prefix("Passed ") {
                    let Some((name, duration)) = result.rsplit_once(" [") else {
                        return fail("malformed .NET passed test result");
                    };
                    if name.is_empty()
                        || name.len() > 1024
                        || name.chars().any(char::is_control)
                        || !duration_pattern.is_match(duration)
                        || !passed.insert(name.to_owned())
                    {
                        return fail("duplicate or malformed .NET passed test result");
                    }
                }
                for (label, target) in [
                    ("Total tests: ", &mut total),
                    ("Passed: ", &mut summary_passed),
                ] {
                    if let Some(value) = row.strip_prefix(label) {
                        let Ok(value) = value.parse::<u64>() else {
                            return fail("invalid .NET summary count");
                        };
                        if target.replace(value).is_some() {
                            return fail("duplicate .NET summary count");
                        }
                    }
                }
                for label in ["Failed: ", "Skipped: "] {
                    if let Some(value) = row.strip_prefix(label) {
                        if value.parse::<u64>().ok() != Some(0)
                            || other_counts.insert(label, 0).is_some()
                        {
                            return fail(".NET summary reports omitted tests or duplicate counts");
                        }
                    }
                }
            }
            // VSTest omits zero Failed/Skipped rows on success. Require the
            // explicit success marker and account for every reported test.
            let count = passed.len() as u64;
            if !successful || count == 0 || total != Some(count) || summary_passed != Some(count) {
                return fail(".NET runner did not prove nonempty complete test execution");
            }
        }
        Runner::Node => {
            let mut totals = BTreeMap::new();
            let mut plan_count = None;
            for line in stdout.lines() {
                if line.starts_with("not ok ") || line.starts_with("Bail out!") {
                    return fail("TAP reports failure");
                }
                if let Some(row) = line.strip_prefix("ok ") {
                    let Some((number, name)) = row.split_once(" - ") else {
                        return fail("invalid TAP result");
                    };
                    if number.parse::<u64>().ok() != Some(passed.len() as u64 + 1)
                        || name.contains(" #")
                        || !passed.insert(name.to_owned())
                    {
                        return fail("skipped, duplicate or malformed TAP result");
                    }
                }
                if let Some(row) = line.strip_prefix("1..") {
                    if plan_count.is_some() {
                        return fail("multiple top-level TAP plans");
                    }
                    plan_count = row.parse::<u64>().ok();
                    if plan_count.is_none() {
                        return fail("invalid TAP plan");
                    }
                }
                for key in ["tests", "pass", "fail", "cancelled", "skipped", "todo"] {
                    if let Some(value) = line.strip_prefix(&format!("# {key} ")) {
                        let Ok(value) = value.parse::<u64>() else {
                            return fail("invalid TAP summary");
                        };
                        if totals.insert(key, value).is_some() {
                            return fail("duplicate TAP summary");
                        }
                    }
                }
            }
            let count = passed.len() as u64;
            if count == 0
                || plan_count != Some(count)
                || totals.get("tests") != Some(&count)
                || totals.get("pass") != Some(&count)
                || ["fail", "cancelled", "skipped", "todo"]
                    .iter()
                    .any(|key| totals.get(key) != Some(&0))
            {
                return fail("TAP did not prove a nonempty, complete, unskipped check");
            }
            // This bounded parser intentionally rejects nested suites until their
            // parent/child accounting is qualified, instead of guessing coverage.
        }
        Runner::Cargo => {
            let mut total = 0u64;
            let mut summaries = 0;
            for line in stdout.lines().chain(stderr.lines()) {
                if let Some(row) = line
                    .strip_prefix("test ")
                    .and_then(|s| s.strip_suffix(" ... ok"))
                {
                    if !passed.insert(row.to_owned()) {
                        return fail("ambiguous duplicate Rust test name");
                    }
                }
                if let Some(row) = line.strip_prefix("test result: ") {
                    let fields: Vec<_> = row.split_whitespace().collect();
                    if fields.len() < 13
                        || fields[0] != "ok."
                        || fields[2] != "passed;"
                        || fields[3] != "0"
                        || fields[4] != "failed;"
                        || fields[5] != "0"
                        || fields[6] != "ignored;"
                        || fields[7] != "0"
                        || fields[8] != "measured;"
                        || fields[9] != "0"
                        || fields[10..13] != ["filtered", "out;", "finished"]
                    {
                        return fail("Rust test summary reports failure or omitted tests");
                    }
                    let Ok(count) = fields[1].parse::<u64>() else {
                        return fail("invalid Rust test count");
                    };
                    let Some(next) = total.checked_add(count) else {
                        return fail("Rust count overflow");
                    };
                    total = next;
                    summaries += 1;
                }
            }
            if summaries == 0 || total == 0 || total != passed.len() as u64 {
                return fail("Rust runner did not prove nonempty test execution");
            }
        }
    }
    if !plan.expected_tests.iter().all(|name| passed.contains(name)) {
        return fail("expected acceptance tests were not observed");
    }
    CheckOutcome::Passed
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn java_python_require_expected_identity_and_successful_process_receipt() {
        for (runner, output, expected) in [
            (Runner::Maven, "[INFO] Running example.Check\n[INFO] Tests run: 1, Failures: 0, Errors: 0, Skipped: 0, Time elapsed: 0.01 s -- in example.Check\n[INFO] example.Check.acceptance -- Time elapsed: 0.01 s\n[INFO] Tests run: 1, Failures: 0, Errors: 0, Skipped: 0\n[INFO] BUILD SUCCESS\n", "example.Check.acceptance"),
            (Runner::Pytest, "collected 1 item\ntest_check.py::test_acceptance PASSED [100%]\n=== 1 passed in 0.01s ===\n", "test_check.py::test_acceptance"),
        ] {
            let mut plan = plan(runner);
            assert!(matches!(evaluate(&plan, Some(0), output.as_bytes(), b"", None), CheckOutcome::Failed { .. }));
            plan.expected_tests = vec![expected.into()];
            assert_eq!(evaluate(&plan, Some(0), output.as_bytes(), b"", None), CheckOutcome::Passed);
            for (code, reason) in [(Some(1), None), (None, None), (Some(0), Some("output truncated"))] {
                assert!(matches!(evaluate(&plan, code, output.as_bytes(), b"", reason), CheckOutcome::Failed { .. }));
            }
        }
    }

    #[test]
    fn maven_configuration_is_explicit_bounded_and_runner_specific() {
        let install = std::env::temp_dir().join("maven-install");
        let launcher = MavenLauncher {
            home: install.to_string_lossy().into_owned(),
            classworlds_jar: install
                .join("boot/plexus-classworlds.jar")
                .to_string_lossy()
                .into_owned(),
            classworlds_conf: install.join("bin/m2.conf").to_string_lossy().into_owned(),
        };
        let mut requirement = Requirement {
            manifest: "pom.xml".into(),
            runner: Runner::Maven,
            profile: "java".into(),
            timeout_ms: None,
            maven: Some(launcher.clone()),
            expected_tests: vec!["example.Check.acceptance".into()],
            rationale: "Owner acceptance".into(),
        };
        assert!(requirement.validate().is_ok());
        requirement.maven = None;
        assert!(requirement.validate().is_err());
        requirement.maven = Some(launcher.clone());
        requirement.runner = Runner::Pytest;
        assert!(requirement.validate().is_err());
        requirement.runner = Runner::Maven;
        for invalid in [
            "relative.jar".into(),
            format!("{};extra.jar", launcher.classworlds_jar),
            install
                .join("boot/../other.jar")
                .to_string_lossy()
                .into_owned(),
            install.join("other.jar").to_string_lossy().into_owned(),
            format!("{}\n", launcher.classworlds_jar),
        ] {
            requirement.maven = Some(MavenLauncher {
                classworlds_jar: invalid,
                ..launcher.clone()
            });
            assert!(requirement.validate().is_err());
        }
    }

    #[test]
    fn dotnet_requires_complete_unskipped_exact_test_evidence() {
        let plan = plan(Runner::Dotnet);
        let output = "  Passed seeded_acceptance [2 ms]\nTest Run Successful.\nTotal tests: 1\n     Passed: 1\n Total time: 0.6910 Seconds\n";
        assert_eq!(
            evaluate(&plan, Some(0), output.as_bytes(), b"", None),
            CheckOutcome::Passed
        );
        for malformed in [
            String::new(),
            output.replace("Total tests: 1", "Total tests: 0"),
            output.replace("Passed: 1", "Passed: 2"),
            output.replace("Test Run Successful.", "Test Run Failed."),
            output.replace("seeded_acceptance", "unrelated"),
            output.replace(" [2 ms]", ""),
            format!("{output}Skipped: 1\n"),
            format!("{output}Failed: 1\n"),
            format!("{output}  Passed seeded_acceptance [1 ms]\n"),
            format!("{output}Total tests: 1\n"),
            format!("{output}Test Run Successful.\n"),
        ] {
            assert!(
                matches!(
                    evaluate(&plan, Some(0), malformed.as_bytes(), b"", None),
                    CheckOutcome::Failed { .. }
                ),
                "{malformed}"
            );
        }
        assert!(matches!(
            evaluate(&plan, Some(1), output.as_bytes(), b"", None),
            CheckOutcome::Failed { .. }
        ));
        assert!(matches!(
            evaluate(&plan, Some(0), output.as_bytes(), b"", Some("timeout")),
            CheckOutcome::Failed { .. }
        ));
    }
    #[test]
    fn dotnet_application_warning_is_not_a_test_result() {
        // Retained B T2 stdout dc8ad34b-7269-4ae0-a5d4-c2de2265de9b:
        // all twelve results passed; HTTPS redirection logged this warning.
        let names = [
            "Inventory.Tests.UnitTest1.Test1",
            "Inventory.Tests.DomainValidationTests.Supplier_requires_valid_email",
            "Inventory.Tests.DomainValidationTests.StockMovement_requires_nonzero_quantity",
            "Inventory.Tests.DomainValidationTests.Product_rejects_invalid_sku",
            "Inventory.Tests.ApiContractTests.Sale_requires_negative_quantity",
            "Inventory.Tests.ApiContractTests.Duplicate_supplier_name_returns_conflict",
            "Inventory.Tests.ApiContractTests.Low_stock_report_is_ordered_by_sku",
            "Inventory.Tests.ApiContractTests.Suppliers_are_ordered_by_name",
            "Inventory.Tests.ApiContractTests.Product_creation_returns_location",
            "Inventory.Tests.ApiContractTests.Unknown_supplier_returns_validation_error",
            "Inventory.Tests.ApiContractTests.Invalid_product_page_returns_bad_request",
            "Inventory.Tests.ApiContractTests.Products_support_search_and_paging",
        ];
        let mut plan = plan(Runner::Dotnet);
        plan.expected_tests = names.iter().map(|name| (*name).into()).collect();
        let mut output = String::from(
            "warn: Microsoft.AspNetCore.HttpsPolicy.HttpsRedirectionMiddleware[3]\r\n      Failed to determine the https port for redirect.\r\n",
        );
        for name in names {
            output.push_str(&format!("  Passed {name} [2 ms]\r\n"));
        }
        output.push_str("Test Run Successful.\r\nTotal tests: 12\r\n     Passed: 12\r\n");
        assert_eq!(
            evaluate(&plan, Some(0), output.as_bytes(), b"", None),
            CheckOutcome::Passed
        );
        // A conflicting actual result still fails even with success counts.
        for failure in [
            "  Failed Inventory.Tests.AdditionalTest [< 1 ms]\r\n",
            "  Skipped Inventory.Tests.AdditionalTest [1 ms]\r\n",
            "Test Run Failed.\r\n",
            "Test Run Aborted.\r\n",
            "Failed: 1\r\n",
            "Skipped: 1\r\n",
        ] {
            for (stdout, stderr) in [
                (format!("{output}{failure}"), String::new()),
                (output.clone(), failure.into()),
            ] {
                assert!(matches!(
                    evaluate(&plan, Some(0), stdout.as_bytes(), stderr.as_bytes(), None),
                    CheckOutcome::Failed { .. }
                ));
            }
        }
        let omitted = output.replace(
            "  Passed Inventory.Tests.UnitTest1.Test1 [2 ms]\r\n",
            "  Skipped Inventory.Tests.UnitTest1.Test1\r\n",
        );
        assert!(matches!(
            evaluate(&plan, Some(0), omitted.as_bytes(), b"", None),
            CheckOutcome::Failed { .. }
        ));
    }
    #[test]
    fn dotnet_solution_discovery_is_literal_bounded_and_fail_closed() {
        let slnx = br#"<Solution><Folder Name="/tests/"><Project Path="tests/Fixture.csproj" /></Folder></Solution>"#;
        assert_eq!(
            dotnet_projects("Fixture.slnx", slnx).unwrap(),
            vec!["tests/Fixture.csproj"]
        );
        let sln = "Microsoft Visual Studio Solution File, Format Version 12.00\nProject(\"{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}\") = \"Fixture\", \"tests\\Fixture.csproj\", \"{EA42D705-5530-40DB-A5A0-57DBE764972E}\"\nEndProject\n";
        assert_eq!(
            dotnet_projects("Fixture.sln", sln.as_bytes()).unwrap(),
            vec!["tests/Fixture.csproj"]
        );
        assert_eq!(
            dotnet_projects("Fixture.sln", format!("\u{feff}\r\n{sln}").as_bytes()).unwrap(),
            vec!["tests/Fixture.csproj"]
        );
        for xml in ["<Solution/>", "<Solution><Project Path=\"../escape.csproj\" /></Solution>", "<Solution><Project Path=\"C:/escape.csproj\" /></Solution>", "<Solution><Project Path=\"fixture.csproj\" /><Project Path=\"FIXTURE.csproj\" /></Solution>", "<Solution><Project Path=\"&external;\" /></Solution>", "<!DOCTYPE Solution [<!ENTITY external SYSTEM 'file:///secret'>]><Solution/>", "<Solution><Unknown/></Solution>", "<Solution><Project Path=\"test.csproj\"></Project></Solution>", "<Solution><Folder Name=\"/tests/\"><Project Path=\"test.csproj\" /></Solution>"] {
            assert!(dotnet_projects("Fixture.slnx", xml.as_bytes()).is_err(), "{xml}");
        }
        assert!(dotnet_projects(
            "Fixture.sln",
            sln.replace("tests\\Fixture.csproj", "..\\Fixture.csproj")
                .as_bytes()
        )
        .is_err());
        assert!(dotnet_projects(
            "Fixture.sln",
            sln.replace("Project(", "Project(garbage").as_bytes()
        )
        .is_err());
    }
    #[test]
    #[cfg(windows)]
    fn dotnet_discovery_requires_observed_projects_and_preserves_scope() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("tests")).unwrap();
        std::fs::write(
            temp.path().join("Fixture.slnx"),
            "<Solution><Project Path=\"tests/Fixture.csproj\" /></Solution>",
        )
        .unwrap();
        let root = vcp_repository::Root::open(
            vcp_repository::RootIdentity {
                workspace: WorkspaceId::parse("workspace").unwrap(),
                root: RootId::parse("root").unwrap(),
                repository: "synthetic".into(),
                worktree: "synthetic".into(),
                binding: Revision::ZERO,
            },
            temp.path(),
        )
        .unwrap();
        let observe = || {
            let scan = root.discover(&Default::default()).unwrap();
            Observation {
                manifest: vcp_repository::observation::Manifest {
                    version: 1,
                    identity: root.identity.clone(),
                    git: None,
                    files: scan.sources.iter().map(|s| s.version.clone()).collect(),
                    ignore_dependencies: scan.ignore_dependencies,
                    exclusions: scan.exclusions,
                    bounded_scan_complete: scan.complete,
                },
                digest: String::new(),
                sources: scan.sources,
                git: None,
            }
        };
        let requirement = Requirement {
            manifest: "Fixture.slnx".into(),
            runner: Runner::Dotnet,
            maven: None,
            profile: "dotnet".into(),
            timeout_ms: Some(120000),
            expected_tests: vec!["Fixture.Checks.Acceptance".into()],
            rationale: "qualified solution acceptance".into(),
        };
        let observation = observe();
        assert!(discover(&observation, &[requirement.clone()]).unwrap()[0]
            .not_run
            .is_some());
        std::fs::write(
            temp.path().join("tests/Fixture.csproj"),
            "<Project Sdk=\"Microsoft.NET.Sdk\" />",
        )
        .unwrap();
        let observation = observe();
        let plans = discover(&observation, &[requirement]).unwrap();
        assert!(plans[0].not_run.is_none());
        assert_eq!(plans[0].directory, "");
        assert_eq!(
            plans[0].request.arguments,
            [
                "test",
                "Fixture.slnx",
                "--no-restore",
                "--nologo",
                "--logger",
                "console;verbosity=normal",
                "--disable-build-servers"
            ]
        );
    }
    fn plan(runner: Runner) -> Plan {
        Plan {
            specification: "project#test".into(),
            origin: None,
            directory: ".".into(),
            runner,
            request: process::Request {
                profile: "test".into(),
                arguments: vec![],
                directory: ".".into(),
                timeout_ms: 1000,
                output_bytes: 1024,
                input: None,
            },
            expected_tests: vec!["seeded_acceptance".into()],
            rationale: "fixture acceptance".into(),
            not_run: None,
        }
    }
    #[test]
    fn verification_requires_expected_tests_and_complete_runner_results() {
        let tap = b"TAP version 13\nok 1 - seeded_acceptance\n1..1\n# tests 1\n# pass 1\n# fail 0\n# cancelled 0\n# skipped 0\n# todo 0\n";
        let node = plan(Runner::Node);
        assert_eq!(
            evaluate(&node, Some(0), tap, b"", None),
            CheckOutcome::Passed
        );
        for output in [b"".as_slice(), b"1..0\n# tests 0\n", b"ok 1 - unrelated\n1..1\n# tests 1\n# pass 1\n# fail 0\n# cancelled 0\n# skipped 0\n# todo 0\n"] {
            assert!(matches!(evaluate(&node, Some(0), output, b"", None), CheckOutcome::Failed { .. }));
        }
        assert!(matches!(
            evaluate(&node, Some(1), tap, b"", None),
            CheckOutcome::Failed { .. }
        ));
        assert!(matches!(
            evaluate(&node, Some(0), tap, b"", Some("timeout")),
            CheckOutcome::Failed { .. }
        ));
        for suffix in ["# pass 1\n", "not ok 2 - failure\n", "1..1\n"] {
            assert!(matches!(
                evaluate(
                    &node,
                    Some(0),
                    &[tap.as_slice(), suffix.as_bytes()].concat(),
                    b"",
                    None
                ),
                CheckOutcome::Failed { .. }
            ));
        }
        let cargo = plan(Runner::Cargo);
        let output = b"running 1 test\ntest seeded_acceptance ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n";
        assert_eq!(
            evaluate(&cargo, Some(0), output, b"", None),
            CheckOutcome::Passed
        );
        assert!(matches!(evaluate(&cargo, Some(0), b"test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n", b"", None), CheckOutcome::Failed { .. }));
        let filtered = String::from_utf8(output.to_vec())
            .unwrap()
            .replace("0 filtered", "1 filtered");
        assert!(matches!(
            evaluate(&cargo, Some(0), filtered.as_bytes(), b"", None),
            CheckOutcome::Failed { .. }
        ));
    }
}
