use std::env;
use std::error::Error as StdError;
use std::fmt;
use std::path::{Path, PathBuf};

/// Represents errors that can occur during Gradle argument generation.
#[derive(Debug)]
pub enum GenerateArgsError {

    /// `JAVA_HOME` was not provided, and the `java` executable could not be found
    /// in the system's `PATH`.
    JavaNotFound,
}

impl fmt::Display for GenerateArgsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GenerateArgsError::JavaNotFound => write!(
                f,
                "ERROR: JAVA_HOME is not set and no 'java' command could be found in your PATH.\n\
                 Please set the JAVA_HOME variable in your environment to match the \
                 location of your Java installation."
            ),
        }
    }
}

impl StdError for GenerateArgsError {}

/// Configuration options for generating Gradle command-line arguments.
///
/// This struct holds all the necessary inputs to replicate the behavior
/// of the standard `gradlew` shell script.
#[derive(Debug)]
pub struct GradleLaunchOptions<'a> {
    /// The path to the JDK installation, equivalent to the `JAVA_HOME` environment variable.
    /// If `None`, the `java` command will be searched for in the system's `PATH`.
    pub jdk_home: Option<&'a Path>,

    /// The application's home directory, which is the directory containing the `gradlew` script.
    pub app_home: &'a Path,

    /// The base name of the script or application being run (e.g., "gradlew").
    /// This is used to set the `org.gradle.appname` system property.
    pub app_base_name: &'a str,

    /// A slice of command-line arguments that were passed to the script.
    pub cli_args: &'a [String],

    /// An optional override for the `GRADLE_OPTS` environment variable.
    /// If `None`, the function will attempt to read it from the environment.
    pub gradle_opts: Option<&'a str>,

    /// An optional override for the `JAVA_OPTS` environment variable.
    /// If `None`, the function will attempt to read it from the environment.
    pub java_opts: Option<&'a str>,
}

/// Generates the Java command and arguments required to launch the Gradle wrapper.
///
/// This function translates the logic of the standard POSIX `gradlew` shell script
/// into a native Rust implementation. It determines the correct Java executable,
/// constructs the classpath, and assembles all JVM options and application arguments.
///
/// ### Comparison with the Shell Script
///
/// This implementation faithfully reproduces the argument generation logic, but differs in
/// a few platform-specific ways:
///
/// - **Path Handling**: It does not perform `cygpath` conversions for Windows compatibility
///   layers like Cygwin or MSYS. A native Rust application uses the appropriate path
///   format for the host OS directly.
/// - **Resource Limits**: It does not set file descriptor limits (the `ulimit` command).
///   This is a process-level setting that should be handled by the caller before
///   executing the generated command, if required.
///
/// # Arguments
///
/// * `options` - A struct containing all necessary parameters, such as the JDK path,
///   application home directory, and command-line arguments.
///
/// # Returns
///
/// A `Result` which, on success, contains a tuple of:
/// * `PathBuf`: The absolute path to the Java executable to be run.
/// * `Vec<String>`: A vector of arguments to pass to the Java executable.
///
/// # Errors
///
/// Returns a `GenerateArgsError` if a valid Java executable cannot be found.
pub fn generate_gradle_args(
    options: &GradleLaunchOptions,
) -> Result<(PathBuf, Vec<String>), GenerateArgsError> {
    // Determine the Java command to use to start the JVM.
    // This logic mimics the script's handling of the `JAVA_HOME` environment variable.
    let java_cmd = match options.jdk_home {
        Some(java_home) => java_home,
        None => {
            // If JAVA_HOME is not set, search for `java` in the system's PATH.
            &which::which("java").map_err(|_| GenerateArgsError::JavaNotFound)?
        }
    };

    // Define constants and derived paths as in the script.
    const DEFAULT_JVM_OPTS: &str = r#""-Xmx64m" "-Xms64m""#;
    let classpath = options
        .app_home
        .join("gradle")
        .join("wrapper")
        .join("gradle-wrapper.jar");

    // Get JVM options from the environment or the provided override options.
    // An empty string is used as a safe default if the environment variable is not set.
    let gradle_opts = options
        .gradle_opts
        .map(String::from)
        .unwrap_or_else(|| env::var("GRADLE_OPTS").unwrap_or_default());
    let java_opts = options
        .java_opts
        .map(String::from)
        .unwrap_or_else(|| env::var("JAVA_OPTS").unwrap_or_default());

    // The shell script uses a complex chain of `printf | xargs | sed | eval` to perform
    // word-splitting on the options string while respecting quotes.
    // The `shlex::split` function is the idiomatic and safe Rust equivalent.
    let all_jvm_opts_str = format!("{} {} {}", DEFAULT_JVM_OPTS, java_opts, gradle_opts);
    let jvm_opts = shlex::split(&all_jvm_opts_str).unwrap_or_else(Vec::new);

    // Collect all arguments for the `java` command in the correct order.
    let mut final_args: Vec<String> = Vec::new();

    // 1. Add the parsed JVM options (`DEFAULT_JVM_OPTS`, `JAVA_OPTS`, `GRADLE_OPTS`).
    final_args.extend(jvm_opts);

    // 2. Add Gradle-specific system properties.
    final_args.push(format!("-Dorg.gradle.appname={}", options.app_base_name));

    // 3. Add the classpath argument.
    final_args.push("-classpath".to_string());
    final_args.push(classpath.to_string_lossy().into_owned());

    // 4. Add the main class to run.
    final_args.push("org.gradle.wrapper.GradleWrapperMain".to_string());

    // 5. Add all original command-line arguments passed to the script.
    final_args.extend_from_slice(options.cli_args);

    Ok((PathBuf::from(java_cmd), final_args))
}