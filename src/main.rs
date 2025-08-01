mod building;
mod config;
mod java;
pub mod utils;

use crate::building::gradle::{GradleLaunchOptions, build_with_gradle};
use crate::config::ProgramParameters;
use crate::java::{Jdk, JdkTrait};
use crate::utils::git::{FastForwardStatus, fast_forward};
use clap::Parser;
use git2::Repository;
use log::{error, info};
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::{env, io, process};
use tokio::fs;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // init logger
    if env::var_os("RUST_LOG").is_none() {
        unsafe {
            env::set_var("RUST_LOG", "info");
        }
    }

    env_logger::init();

    let mut base_dir = PathBuf::new();

    base_dir.push(env::home_dir().unwrap());
    base_dir.push(".cubewhy");
    base_dir.push("lunarcn");
    base_dir.push("bootstrap-next");

    let mut javaagent_dir = PathBuf::new();
    javaagent_dir.push(env::home_dir().unwrap());
    javaagent_dir.push(".cubewhy");
    javaagent_dir.push("lunarcn");
    javaagent_dir.push("javaagents");

    // parse args
    let args = ProgramParameters::parse();

    info!("Welcome to Celestial Bootstrap Next!");

    let Some(jdk) = Jdk::resolve_higher(17).await else {
        error!("Celestial requires Jdk 17 or higher to run, please download one manually.");
        process::exit(1);
    };

    info!(
        "Use Jdk {} {}",
        jdk.version(),
        jdk.java_executable().to_string_lossy()
    );

    let celestial_jar_path = base_dir.join("celestial.jar");
    let debugger_jar_path = javaagent_dir.join("browser-debugger.jar");

    let is_first_run = !fs::try_exists(&celestial_jar_path).await?;

    // update Celestial
    info!("Check update for Celestial Launcher");
    match check_update(
        &base_dir.join("repositories").join("celestial"),
        "https://codeberg.org/earthsworth/celestial.git",
        &args.celestial_branch,
        &celestial_jar_path,
        &jdk,
    )
    .await
    {
        Ok(_) => (),
        Err(err) => {
            log_backtrace!("Failed to update Celestial! {}", err);
            process::exit(1);
        }
    }
    // Update Browser Debugger
    if fs::try_exists(&debugger_jar_path).await? || is_first_run {
        info!("Check update for Browser Debugger");
        match check_update(
            &base_dir.join("repositories").join("browser-debugger"),
            "https://codeberg.org/earthsworth/BrowserDebugger.git",
            &args.debugger_branch,
            &debugger_jar_path,
            &jdk,
        )
        .await
        {
            Ok(_) => (),
            Err(err) => {
                log_backtrace!("Failed to update BrowserDebugger! {}", err);
                process::exit(1);
            }
        }
    } else {
        info!("Skipped check update for Browser Debugger: user manually removed the agent");
    }

    // spawn celestial
    info!("Spawning Celestial Launcher");
    if let Ok(status) = spawn_jar(&jdk, &celestial_jar_path).await {
        if status.success() {
            info!("Celestial launcher terminated.");
        } else {
            error!(
                "Celestial launcher terminated with a non-zero exit code: {}",
                status.code().unwrap_or(-1)
            );
        }
    };

    Ok(())
}

async fn spawn_jar(java: &impl JdkTrait, jar_path: &Path) -> io::Result<ExitStatus> {
    let mut command = tokio::process::Command::new(java.java_executable());
    command.arg("-jar");
    command.arg(jar_path);

    // spawn command
    let mut child = command.spawn()?;
    child.wait().await
}

async fn check_update(
    repo_path: &Path,
    repo: &str,
    branch: &str,
    emitted_jar_path: &Path,
    jdk: &impl JdkTrait,
) -> anyhow::Result<()> {
    let branch = branch.to_string();
    let repo_path = repo_path.to_owned();
    let repo = repo.to_string();
    // (repo, should (re-)build jar)
    let (repo, should_build): (Repository, bool) = tokio::task::spawn_blocking(move || {
        // TODO: checkout branch/commit
        if repo_path.is_dir() {
            // try to open the repository
            return match Repository::open(&repo_path) {
                Ok(repo) => {
                    // try to pull the repository
                    match fast_forward(&repo, &branch) {
                        Ok(status) => {
                            return Ok((repo, status == FastForwardStatus::FastForward));
                        }
                        Err(err) => error!("Failed to pull repository: {err}"),
                    }
                    if let Err(err) = fast_forward(&repo, &branch) {
                        // it's ok failed to pull repository
                        error!("Failed to pull repository: {err}");
                    }
                    Ok((repo, false))
                }
                Err(e) => Err(e),
            };
        }
        // repository not found
        // clone the repository
        info!("Cloning from repository {repo}");
        let repo = match Repository::clone(&repo, &repo_path) {
            Ok(repo) => repo,
            Err(e) => return Err(e),
        };
        Ok((repo, true))
    })
    .await?
    .map_err(|err| {
        anyhow::Error::msg(format!(
            "Failed to clone/open repository: {}",
            err.to_string()
        ))
    })?;

    let repo_path = repo.path().parent().unwrap();
    let should_build = should_build || !fs::try_exists(emitted_jar_path).await?;

    // build with gradle
    if should_build {
        info!("Building Celestial");
        build_with_gradle(jdk, &repo_path, emitted_jar_path, "-fatjar").await?;
    }

    Ok(())
}
