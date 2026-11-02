use crate::{engine, model, ui, update};
use clap::{Parser, Subcommand};
use std::fs::{self, OpenOptions};
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "terragrunt-tui",
    about = "Local, read-only Terragrunt plan review",
    version = VERSION
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Inspect a plan using only `terragrunt run --all plan`.
    Run {
        #[arg(long, default_value = ".")]
        cwd: PathBuf,
        #[arg(long, default_value = "terragrunt")]
        terragrunt: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Open a previously saved Terragrunt TUI result.
    Show {
        #[arg(long, default_value = ".terragrunt-tui/latest.json")]
        run: PathBuf,
    },
    /// Install the latest GitHub release (the latest release candidate with `--rc`).
    Update {
        #[arg(long)]
        rc: bool,
    },
    /// Print the installed version.
    Version,
}

/// Version embedded by `build.rs`.
const VERSION: &str = env!("TTUI_VERSION");

/// Entry point shared by the `terragrunt-tui` and `ttui` binaries.
pub fn main() {
    let code = match execute(Cli::parse()) {
        Ok(code) => code,
        Err(error)
            if error
                .downcast_ref::<io::Error>()
                .is_some_and(|error| error.kind() == io::ErrorKind::Interrupted) =>
        {
            130
        }
        Err(error) => {
            eprintln!("terragrunt-tui: {error}");
            2
        }
    };
    std::process::exit(code);
}

fn execute(cli: Cli) -> Result<i32, Box<dyn std::error::Error>> {
    match cli.command {
        Command::Run {
            cwd,
            terragrunt,
            output,
        } => run(cwd, &terragrunt, output),
        Command::Show { run } => show(run),
        Command::Update { rc } => update::run(rc),
        Command::Version => {
            println!("terragrunt-tui {VERSION}");
            Ok(0)
        }
    }
}

fn run(
    cwd: PathBuf,
    terragrunt: &str,
    output: Option<PathBuf>,
) -> Result<i32, Box<dyn std::error::Error>> {
    let root = cwd.canonicalize()?;
    if !root.is_dir() {
        return Err(format!("working directory is not a directory: {}", root.display()).into());
    }
    cleanup_stale_if_confirmed()?;
    let units = engine::discover(terragrunt, &root)?;
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("interactive terminal required for the explicit pre-plan confirmation; no plan was executed".into());
    }
    let workspace = engine::RunWorkspace::new()?;
    let command = workspace.command(terragrunt);
    let display_command = command
        .iter()
        .map(|part| serde_json::to_string(part).unwrap_or_else(|_| "\"?\"".into()))
        .collect::<Vec<_>>()
        .join(" ");
    let mut session = ui::TerminalSession::new()?;
    let confirmed = ui::confirm_plan(
        &mut session.terminal,
        &root.display().to_string(),
        &display_command,
        &units,
        &workspace.path().display().to_string(),
    )?;
    if !confirmed {
        return Ok(0);
    }

    let mut completed = 0usize;
    let mut current = None;
    let mut logs = Vec::new();
    let mut progress_error = None;
    let result = engine::execute_plan(terragrunt, &root, &units, &workspace, &mut |event| {
        if let Some(path) = event.unit {
            completed = completed.saturating_add(1);
            current = Some(path);
        }
        if let Some(line) = event.line {
            logs.push(line);
        }
        match ui::show_progress(
            &mut session.terminal,
            current.as_deref(),
            completed,
            units.len(),
            &logs,
        ) {
            Ok(keep_running) => keep_running,
            Err(error) => {
                progress_error = Some(error);
                false
            }
        }
    })?;
    drop(workspace);
    if let Some(error) = progress_error {
        return Err(error.into());
    }
    let output_path = output.unwrap_or_else(|| PathBuf::from(".terragrunt-tui/latest.json"));
    persist_and_review(
        || save_result(&output_path, &result),
        || ui::review(&mut session.terminal, &result).map_err(Into::into),
    )?;
    Ok(if result.complete {
        0
    } else if result.exit_code != 0 {
        result.exit_code
    } else {
        2
    })
}

fn cleanup_stale_if_confirmed() -> Result<(), Box<dyn std::error::Error>> {
    let stale = engine::stale_temp_dirs();
    if stale.is_empty() {
        return Ok(());
    }
    let paths = stale
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        eprintln!(
            "Terragrunt TUI found marked temporary directories; no automatic deletion. Review them with an interactive run: {paths:?}"
        );
        return Ok(());
    }
    let mut session = ui::TerminalSession::new()?;
    if ui::confirm_cleanup(&mut session.terminal, &paths)? {
        for path in stale {
            engine::remove_stale_temp(&path)?;
        }
    }
    Ok(())
}

fn show(path: PathBuf) -> Result<i32, Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read result {}: {error}", path.display()))?;
    let result: model::RunResult = serde_json::from_str(&contents)
        .map_err(|error| format!("cannot parse result {}: {error}", path.display()))?;
    if result.schema_version != 1 {
        return Err(format!(
            "unsupported result schema version {}",
            result.schema_version
        )
        .into());
    }
    if io::stdin().is_terminal() && io::stdout().is_terminal() {
        let mut session = ui::TerminalSession::new()?;
        ui::review(&mut session.terminal, &result)?;
    } else {
        print_summary(&result);
    }
    Ok(if result.complete {
        0
    } else if result.exit_code != 0 {
        result.exit_code
    } else {
        2
    })
}

fn save_result(path: &Path, result: &model::RunResult) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            format!(
                "cannot create result {} without overwriting: {error}",
                path.display()
            )
        })?;
    serde_json::to_writer_pretty(file, result)?;
    Ok(())
}

fn persist_and_review<S, R>(save: S, review: R) -> Result<(), Box<dyn std::error::Error>>
where
    S: FnOnce() -> Result<(), Box<dyn std::error::Error>>,
    R: FnOnce() -> Result<(), Box<dyn std::error::Error>>,
{
    let save_error = save().err();
    review()?;
    if let Some(error) = save_error {
        return Err(error);
    }
    Ok(())
}

fn print_summary(result: &model::RunResult) {
    if result.cancelled {
        println!("ANNULE / INCOMPLET · totaux globaux masqués");
    } else if let Some(totals) = &result.totals {
        println!(
            "COMPLET · ADD {} · CHANGE {} · DELETE {}",
            totals.added, totals.changed, totals.deleted
        );
    } else {
        println!("INCOMPLET / ILLISIBLE · totaux globaux masqués");
    }
    for module in &result.modules {
        let wave = module
            .wave
            .map_or_else(|| "?".into(), |wave| wave.to_string());
        println!(
            "[W{wave}] {} · {}",
            module.path,
            module.status.to_uppercase()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_result_file_does_not_prevent_review() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("latest.json");
        fs::write(&output, "previous-result").unwrap();
        let result = model::RunResult {
            schema_version: 1,
            command: vec![
                "terragrunt".into(),
                "run".into(),
                "--all".into(),
                "plan".into(),
            ],
            exit_code: 0,
            complete: true,
            cancelled: false,
            totals: Some(model::Totals {
                added: 1,
                changed: 0,
                deleted: 0,
            }),
            modules: Vec::new(),
        };
        let mut reviewed = false;

        let outcome = persist_and_review(
            || save_result(&output, &result),
            || {
                reviewed = true;
                Ok(())
            },
        );

        assert!(reviewed, "the result review must run after save failure");
        assert!(outcome.is_err(), "the save failure must remain visible");
        assert_eq!(fs::read_to_string(output).unwrap(), "previous-result");
    }
}
