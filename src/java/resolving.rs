use std::io;
use std::path::Path;
use once_cell::sync::Lazy;
use regex::Regex;
use thiserror::Error;

/// A regular expression to capture the version string from `java -version` output.
/// It's designed to match common formats from OpenJDK, Oracle Java, etc.
/// Example: `openjdk version "11.0.12"` -> captures `11.0.12`
/// Example: `java version "1.8.0_292"` -> captures `1.8.0_292`
static JAVA_VERSION_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?:java|openjdk) version "([^"]+)""#).unwrap()
});

/// Represents errors that can occur while resolving the Java version.
#[derive(Debug, Error)]
pub enum JavaVersionError {
    /// The `java` command could not be found or executed.
    #[error("Java command failed to start.")]
    CommandIo(#[from] io::Error),

    /// The `java -version` command returned a non-zero exit code.
    /// The output from stderr is included for debugging.
    #[error("Java command exited with an error: {0}")]
    CommandFailed(String),

    /// The output of `java -version` was not valid UTF-8.
    #[error("Failed to parse Java command output as UTF-8.")]
    OutputParseError(#[from] std::string::FromUtf8Error),

    /// The version string could not be found in the command's output.
    #[error("Could not find a version string in the output of 'java -version'.")]
    VersionNotFound,
}

/// Asynchronously resolves the installed Java version by executing `java -version`.
///
/// This function is cross-platform and handles the nuance that `java -version`
/// prints its output to `stderr` instead of `stdout`.
///
/// It uses a regular expression to parse the output and extract the version string,
/// such as "11.0.12" or "1.8.0_301".
///
/// # Returns
///
/// A `Result` which, on success, contains the `String` representation of the Java version.
/// On failure, it returns a `JavaVersionError` detailing the cause.
///
/// # Examples
///
/// ```rust
/// // This example requires a tokio runtime.
/// // #[tokio::main]
/// // async fn main() {
/// //     match resolve_java_version().await {
/// //         Ok(version) => println!("Detected Java version: {}", version),
/// //         Err(e) => eprintln!("Error resolving Java version: {}", e),
/// //     }
/// // }
/// ```
pub async fn resolve_java_version(program_path: &Path) -> Result<String, JavaVersionError> {
    // Execute the `java -version` command asynchronously.
    // The `tokio::process::Command` is the async equivalent of `std::process::Command`.
    let output = tokio::process::Command::new(program_path)
        .arg("-version")
        .output()
        .await?; // The `?` propagates any I/O error (e.g., command not found).

    // `java -version` prints to stderr on success. If the command fails for other
    // reasons, it might also print to stderr. We check the exit status first.
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(JavaVersionError::CommandFailed(stderr));
    }

    // Convert the stderr output to a string.
    let stderr = String::from_utf8(output.stderr)?;

    // Use the pre-compiled regex to find the version string.
    // The `captures` method returns an `Option`.
    match JAVA_VERSION_REGEX.captures(&stderr) {
        Some(captures) => {
            // The first capture group (`.get(1)`) contains the version number.
            // `.get(0)` would be the entire matched string (e.g., `java version "1.8.0"`).
            // This unwrap is safe because a successful regex match guarantees the capture group exists.
            let version = captures.get(1).unwrap().as_str();
            Ok(version.to_string())
        }
        None => {
            // If the regex does not match, return the `VersionNotFound` error.
            Err(JavaVersionError::VersionNotFound)
        }
    }
}