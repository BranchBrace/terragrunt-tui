use crate::graph::{DiscoveryError, Unit, dependency_waves, parse_units};
use crate::model::{ModuleResult, RunResult, parse_plan_json, summarize_modules};
use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tempfile::{Builder, TempDir};

#[derive(Debug)]
pub enum EngineError {
    Io(std::io::Error),
    Discovery(DiscoveryError),
    Invalid(String),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "execution error: {error}"),
            Self::Discovery(error) => error.fmt(formatter),
            Self::Invalid(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<std::io::Error> for EngineError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<DiscoveryError> for EngineError {
    fn from(value: DiscoveryError) -> Self {
        Self::Discovery(value)
    }
}

pub struct RunWorkspace {
    temporary: TempDir,
    pub plan_dir: PathBuf,
    pub json_dir: PathBuf,
}

impl RunWorkspace {
    pub fn new() -> Result<Self, EngineError> {
        let temporary = Builder::new().prefix("wayfinder-").tempdir()?;
        fs::write(
            temporary.path().join(".wayfinder-owned"),
            "WAYFINDER_TEMP_V1\n",
        )?;
        let plan_dir = temporary.path().join("plans");
        let json_dir = temporary.path().join("json");
        Ok(Self {
            temporary,
            plan_dir,
            json_dir,
        })
    }

    pub fn command(&self, terragrunt: &str) -> Vec<String> {
        exact_plan_command(terragrunt, &self.plan_dir, &self.json_dir)
    }

    pub fn path(&self) -> &Path {
        self.temporary.path()
    }
}

pub fn discover(terragrunt: &str, root: &Path) -> Result<Vec<Unit>, EngineError> {
    let output = Command::new(terragrunt)
        .args([
            "find",
            "--dependencies",
            "--queue-construct-as=plan",
            "--format=json",
        ])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err(EngineError::Invalid(format!(
            "Terragrunt unit discovery failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(parse_units(&String::from_utf8_lossy(&output.stdout))?)
}

pub fn exact_plan_command(terragrunt: &str, plan_dir: &Path, json_dir: &Path) -> Vec<String> {
    vec![
        terragrunt.to_owned(),
        "run".into(),
        "--all".into(),
        "--out-dir".into(),
        plan_dir.to_string_lossy().into_owned(),
        "--json-out-dir".into(),
        json_dir.to_string_lossy().into_owned(),
        "plan".into(),
    ]
}

pub fn stale_temp_dirs() -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("wayfinder-"))
                && !path.is_symlink()
                && path.is_dir()
                && matches!(
                    fs::read_to_string(path.join(".wayfinder-owned")).as_deref(),
                    Ok("WAYFINDER_TEMP_V1\n")
                )
        })
        .collect()
}

pub fn remove_stale_temp(path: &Path) -> Result<(), EngineError> {
    let root = std::env::temp_dir().canonicalize()?;
    let candidate = path.canonicalize()?;
    if candidate.parent() != Some(root.as_path())
        || !candidate
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("wayfinder-"))
        || path.is_symlink()
        || fs::read_to_string(candidate.join(".wayfinder-owned"))? != "WAYFINDER_TEMP_V1\n"
    {
        return Err(EngineError::Invalid(
            "refusing to remove a directory not owned by Wayfinder".into(),
        ));
    }
    fs::remove_dir_all(candidate)?;
    Ok(())
}

pub fn execute_plan<F>(
    terragrunt: &str,
    root: &Path,
    units: &[Unit],
    workspace: &RunWorkspace,
    progress: &mut F,
) -> Result<RunResult, EngineError>
where
    F: FnMut(Option<String>) -> bool,
{
    let root = root.canonicalize()?;
    fs::create_dir_all(&workspace.plan_dir)?;
    fs::create_dir_all(&workspace.json_dir)?;
    let plan_dir = &workspace.plan_dir;
    let json_dir = &workspace.json_dir;
    let command = workspace.command(terragrunt);
    let mut command_runner = Command::new(terragrunt);
    command_runner
        .args(["run", "--all", "--out-dir"])
        .arg(plan_dir)
        .arg("--json-out-dir")
        .arg(json_dir)
        .arg("plan")
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command_runner.process_group(0);
    }
    let mut child = command_runner.spawn()?;
    let (sender, receiver) = mpsc::channel();
    let mut readers = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        readers.push(spawn_reader(stdout, sender.clone()));
    }
    if let Some(stderr) = child.stderr.take() {
        readers.push(spawn_reader(stderr, sender.clone()));
    }
    drop(sender);
    let mut log_lines = Vec::new();
    let mut reported_units = BTreeSet::new();
    let mut cancelled = !progress(None);
    while !cancelled {
        let mut active = None;
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => {
                if let Some(unit) = units.iter().find(|unit| line.contains(&unit.path))
                    && reported_units.insert(unit.path.clone())
                {
                    active = Some(unit.path.clone());
                }
                log_lines.push(line);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if !progress(active) {
            cancelled = true;
        }
    }
    if cancelled {
        terminate_process_tree(&mut child)?;
    }
    for reader in readers {
        let _ = reader.join();
    }
    let status = child.wait()?;
    let exit_code = if cancelled {
        130
    } else {
        status.code().unwrap_or(2)
    };
    let waves = dependency_waves(&root, units);
    let mut modules = Vec::with_capacity(units.len());
    let mut parsed_count = 0usize;
    for unit in units {
        let plan_file = plan_file_path(json_dir, &root, &unit.path);
        let wave_key = if Path::new(&unit.path).is_absolute() {
            normalize_path(Path::new(&unit.path))
        } else {
            normalize_path(&root.join(&unit.path))
        };
        let wave = waves.get(&wave_key).copied().flatten();
        let parsed = plan_file
            .as_ref()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|contents| parse_plan_json(&contents).ok());
        if let Some((resources, totals)) = parsed {
            parsed_count += 1;
            let has_changes = resources
                .iter()
                .any(|resource| !matches!(resource.kind(), crate::model::ChangeKind::NoChange));
            modules.push(ModuleResult {
                path: unit.path.clone(),
                status: if has_changes { "planned" } else { "no_changes" }.into(),
                wave,
                totals: Some(totals),
                resources,
                logs: matching_logs(&log_lines, &unit.path),
            });
        } else {
            modules.push(ModuleResult {
                path: unit.path.clone(),
                status: if exit_code == 0 { "unknown" } else { "failed" }.into(),
                wave,
                totals: None,
                resources: Vec::new(),
                logs: matching_logs(&log_lines, &unit.path),
            });
        }
    }
    let complete = exit_code == 0 && parsed_count == units.len() && !units.is_empty();
    let totals = if complete {
        summarize_modules(&modules)
    } else {
        None
    };
    Ok(RunResult {
        schema_version: 1,
        command,
        exit_code,
        complete,
        cancelled,
        totals,
        modules,
    })
}

fn terminate_process_tree(child: &mut Child) -> Result<(), std::io::Error> {
    #[cfg(unix)]
    {
        let process_group = -(child.id() as libc::pid_t);
        unsafe {
            libc::kill(process_group, libc::SIGTERM);
        }
        let grace_deadline = std::time::Instant::now() + Duration::from_millis(500);
        loop {
            let _ = child.try_wait()?;
            let group_exists = unsafe { libc::kill(process_group, 0) == 0 };
            if !group_exists {
                break;
            }
            if std::time::Instant::now() >= grace_deadline {
                unsafe {
                    libc::kill(process_group, libc::SIGKILL);
                }
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
    #[cfg(windows)]
    {
        let pid = child.id().to_string();
        let status = Command::new("taskkill")
            .args(["/PID", &pid, "/T", "/F"])
            .status()?;
        if !status.success() {
            child.kill()?;
        }
    }
    #[cfg(not(any(unix, windows)))]
    child.kill()?;
    child.wait()?;
    Ok(())
}

fn spawn_reader<R: Read + Send + 'static>(
    stream: R,
    sender: mpsc::Sender<String>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for line in BufReader::new(stream).lines() {
            let line = line.unwrap_or_else(|error| format!("[output read error: {error}]"));
            if sender.send(line).is_err() {
                break;
            }
        }
    })
}

fn matching_logs(lines: &[String], unit_path: &str) -> Vec<String> {
    lines
        .iter()
        .filter(|line| line.contains(unit_path))
        .cloned()
        .collect()
}

fn plan_file_path(json_dir: &Path, root: &Path, unit_path: &str) -> Option<PathBuf> {
    let unit = Path::new(unit_path);
    let absolute = if unit.is_absolute() {
        unit.to_path_buf()
    } else {
        root.join(unit)
    };
    let normalized = PathBuf::from(normalize_path(&absolute));
    let normalized_root = PathBuf::from(normalize_path(root));
    let relative = normalized.strip_prefix(normalized_root).ok()?;
    let candidate = json_dir.join(relative).join("tfplan.json");
    candidate.starts_with(json_dir).then_some(candidate)
}

fn normalize_path(path: &Path) -> String {
    let mut output = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                output.pop();
            }
            other => output.push(other.as_os_str()),
        }
    }
    output.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_invocation_is_fixed_to_run_all_plan() {
        let args = exact_plan_command(
            "terragrunt",
            Path::new("/tmp/plans"),
            Path::new("/tmp/json"),
        );
        assert_eq!(
            args,
            [
                "terragrunt",
                "run",
                "--all",
                "--out-dir",
                "/tmp/plans",
                "--json-out-dir",
                "/tmp/json",
                "plan"
            ]
        );
        assert!(
            !args
                .iter()
                .any(|argument| argument == "apply" || argument == "destroy")
        );
    }

    #[cfg(unix)]
    fn fake_terragrunt(tools: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let binary = tools.join("terragrunt-fake");
        fs::write(
            &binary,
            r##"#!/usr/bin/env python3
import json, pathlib, subprocess, sys, time
args = sys.argv[1:]
if args[0] == "find":
    print('[{"type":"unit","path":"prod/app","dependencies":[]}]')
elif args[0] == "run":
    output = pathlib.Path(args[args.index("--json-out-dir") + 1]) / "prod/app/tfplan.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps({"resource_changes":[{"address":"demo_item.secret","change":{"actions":["create"],"before":None,"after":{"token":"plaintext-secret"},"after_sensitive":{"token":True}}}]}))
    if pathlib.Path("SLOW").exists():
        sleeper = subprocess.Popen(["sleep", "30"])
        pathlib.Path("CHILD_PID").write_text(str(sleeper.pid))
    print("Running command in prod/app", flush=True)
    if pathlib.Path("SLOW").exists(): time.sleep(30)
else:
    raise SystemExit(90)
"##,
        )
        .unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        binary
    }

    #[cfg(unix)]
    #[test]
    fn synthetic_run_parses_redacts_and_cleans_up_without_applying() {
        let fixture = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        let binary = fake_terragrunt(tools.path());
        let terragrunt = binary.to_string_lossy().to_string();
        let units = discover(&terragrunt, fixture.path()).unwrap();
        let workspace = RunWorkspace::new().unwrap();
        let workspace_path = workspace.path().to_path_buf();
        let mut progress = Vec::new();
        let result = execute_plan(
            &terragrunt,
            fixture.path(),
            &units,
            &workspace,
            &mut |unit| {
                if let Some(unit) = unit {
                    progress.push(unit);
                }
                true
            },
        )
        .unwrap();

        assert!(result.complete);
        assert_eq!(result.totals.as_ref().unwrap().added, 1);
        assert_eq!(result.modules[0].resources[0].after["token"], "[REDACTED]");
        assert_eq!(progress, vec!["prod/app"]);
        assert!(
            !serde_json::to_string(&result)
                .unwrap()
                .contains("plaintext-secret")
        );
        assert_eq!(result.command.last().map(String::as_str), Some("plan"));
        drop(workspace);
        assert!(!workspace_path.exists());
    }

    #[test]
    fn stale_temp_cleanup_only_removes_marked_wayfinder_directories() {
        let marked = Builder::new()
            .prefix("wayfinder-marked-test-")
            .tempdir_in(std::env::temp_dir())
            .unwrap();
        fs::write(
            marked.path().join(".wayfinder-owned"),
            "WAYFINDER_TEMP_V1\n",
        )
        .unwrap();
        let unmarked = Builder::new()
            .prefix("wayfinder-unmarked-test-")
            .tempdir_in(std::env::temp_dir())
            .unwrap();

        let stale = stale_temp_dirs();
        assert!(stale.contains(&marked.path().to_path_buf()));
        assert!(!stale.contains(&unmarked.path().to_path_buf()));
        remove_stale_temp(marked.path()).unwrap();
        assert!(!marked.path().exists());
        assert!(unmarked.path().exists());
    }

    #[cfg(unix)]
    #[test]
    fn cancelling_stops_the_child_and_marks_the_result_incomplete() {
        let fixture = tempfile::tempdir().unwrap();
        fs::write(fixture.path().join("SLOW"), "").unwrap();
        let tools = tempfile::tempdir().unwrap();
        let binary = fake_terragrunt(tools.path());
        let terragrunt = binary.to_string_lossy().to_string();
        let units = discover(&terragrunt, fixture.path()).unwrap();
        let workspace = RunWorkspace::new().unwrap();
        let started = std::time::Instant::now();
        let result = execute_plan(
            &terragrunt,
            fixture.path(),
            &units,
            &workspace,
            &mut |unit| unit.is_none(),
        )
        .unwrap();

        assert!(result.cancelled);
        assert!(!result.complete);
        assert_eq!(result.exit_code, 130);
        assert!(started.elapsed() < Duration::from_secs(5));
        #[cfg(target_os = "linux")]
        {
            let child_pid = fs::read_to_string(fixture.path().join("CHILD_PID"))
                .unwrap()
                .parse::<u32>()
                .unwrap();
            let stopped = (0..25).any(|_| {
                let path = format!("/proc/{child_pid}/stat");
                let result = fs::read_to_string(path)
                    .ok()
                    .and_then(|stat| stat.rsplit_once(')').map(|(_, rest)| rest.to_owned()))
                    .and_then(|rest| rest.split_whitespace().next().map(str::to_owned));
                if result
                    .as_deref()
                    .is_none_or(|state| state == "Z" || state == "X")
                {
                    true
                } else {
                    thread::sleep(Duration::from_millis(20));
                    false
                }
            });
            assert!(
                stopped,
                "descendant process remained active after cancellation"
            );
        }
    }
}
