mod building;
mod config;
mod java;
pub mod utils;

use crate::building::gradle::{generate_gradle_args, GradleLaunchOptions};
use crate::config::ProgramParameters;
use crate::java::{Jdk, JdkTrait};
use crate::utils::git::{fast_forward, FastForwardStatus};
use clap::Parser;
use git2::Repository;
use log::{error, info};
use std::path::{Path, PathBuf};
use std::{env, io, process};
use std::process::ExitStatus;
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

    // parse args
    let args = ProgramParameters::parse();

    info!("Welcome to Celestial Bootstrap Next!");

    let Some(jdk) = Jdk::resolve_higher(17).await else {
        error!("Celestial requires Jdk 17 or higher to run, please download one manually.");
        process::exit(1);
    };

    info!("Use Jdk {} {}", jdk.version(), jdk.java_executable().to_string_lossy());

    let celestial_jar_path = base_dir.join("celestial.jar");

    match check_update_for_celestial(
        &base_dir,
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

    // spawn celestial
    if let Ok(status) = spawn_jar(&jdk, &celestial_jar_path).await {
        if status.success() {
        info!("Celestial launcher terminated.");
        } else {
            error!("Celestial launcher terminated with a non-zero exit code: {}", status.code().unwrap_or(-1));
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

async fn check_update_for_celestial(
    base_dir: &Path,
    repo: &str,
    branch: &str,
    emitted_jar_path: &Path,
    jdk: &impl JdkTrait,
) -> anyhow::Result<()> {
    let repo_path = base_dir.join("repositories").join("celestial");

    let branch = branch.to_string();
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
                        Err(err) => error!("Failed to pull celestial repository: {err}"),
                    }
                    if let Err(err) = fast_forward(&repo, &branch) {
                        // it's ok failed to pull repository
                        error!("Failed to pull celestial repository: {err}");
                    }
                    Ok((repo, false))
                }
                Err(e) => Err(e),
            };
        }
        // repository not found
        // clone the repository
        info!("Cloning Celestial from repository {repo}");
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
        let gradle_run_cmd = generate_gradle_args(&GradleLaunchOptions {
            jdk_home: Some(jdk.java_executable()),
            app_home: repo_path,
            app_base_name: "gradlew",

            cli_args: &["build".to_string()],
            gradle_opts: None,
            java_opts: None,
        })?;

        // spawn celestial process
        info!("Building Celestial");

        // do cleanup first
        let build_libs_dir = repo_path.join("build").join("libs");

        if fs::try_exists(&build_libs_dir).await? {
            fs::remove_dir_all(&build_libs_dir).await?;
        }

        info!("Spawning gradle: {}", gradle_run_cmd.1.join(" "));

        let mut command = tokio::process::Command::new(&gradle_run_cmd.0);
        command.args(gradle_run_cmd.1);
        command.current_dir(&repo_path);
        let mut child = command.spawn()?;

        // wait for build thread
        child.wait().await?;
        info!("Gradle built successfully");

        // locate emitted .jar file
        while let Some(file) = fs::read_dir(&build_libs_dir).await?.next_entry().await? {
            let file_name = file.file_name();
            let file_name: String = file_name.to_string_lossy().into();
            if file_name.contains("-fatjar") {
                if fs::try_exists(emitted_jar_path).await? {
                    // remove this file
                    info!("Remove exist jar {}", emitted_jar_path.display());
                    fs::remove_file(emitted_jar_path).await?;
                }
                // move file
                let built_jar = file.path();
                info!(
                    "Move built jar {} to {}",
                    built_jar.display(),
                    emitted_jar_path.display()
                );
                fs::rename(built_jar, emitted_jar_path).await?;
                break;
            }
        }
        info!("Complete updated Celestial launcher");
    }

    Ok(())
}
