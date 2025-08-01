use std::{env, io, path::PathBuf};

use tokio::fs;

use crate::utils::timestamp::current_unix_timestamp_in_ms;

/// An async implementation of [tempfile::tempfile]
/// Please note that: you need to handle the delete action after you use the file
pub async fn tempfile() -> io::Result<(fs::File, PathBuf)> {
    let timestamp_in_ms = current_unix_timestamp_in_ms();

    let path = env::temp_dir().join(format!("celestial-rs-{timestamp_in_ms}.tmp"));
    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(&path)
        .await?;
    Ok((file, path))
}
