use async_zip::error::ZipError;
use digest::DynDigest;
use futures_util::StreamExt;
use log::error;
use md5::Digest;
use reqwest::Client;
use std::{backtrace::Backtrace, ops::Range, path::PathBuf, sync::Arc};
use tokio::{
    fs::{self, File},
    io::{AsyncWriteExt, BufReader},
    sync::mpsc,
};

use thiserror::Error;

use crate::utils::{
    hashing::{Hash, HashingError},
    stream::stream_write_and_calculate_hash,
    tempfile_async,
};

#[derive(Error, Debug)]
pub enum DownloadError {
    #[error("Failed to calculate hashcode")]
    Hashing(#[from] HashingError),

    #[error("Failed to unzip")]
    Unarchive(#[from] ZipError),

    #[error("Error fetching file")]
    Http(#[from] reqwest::Error),

    #[error("IO Error")]
    Io(#[from] std::io::Error),

    #[error("Max retries exceeded when requesting to URL {url} (max_retries = {max_retries})")]
    MaxRetriesExceeded { url: String, max_retries: u32 },

    #[error("Failed to create parent folders of the path {0}")]
    FailedCreateParentFolders(PathBuf),
}

pub async fn download_parallelly(
    client: &Client,
    url: &str,
    file: &mut File,
    expected_file_hash: Option<&Hash>,
    concurrency: usize,
    max_retries: u32,
) -> Result<(), DownloadError> {
    // fetch file size
    let res = client.head(url).send().await?;
    let total_size: Option<usize> = res
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());

    let Some(total_size) = total_size else {
        // download in the single thread since Celestial cannot know the size of the file
        return download_single_thread(client, url, file, expected_file_hash, max_retries).await;
    };

    if total_size <= 5120 {
        // file too small, do parallel download is expensize
        return download_single_thread(client, url, file, expected_file_hash, max_retries).await;
    }

    let concurrency = if concurrency >= total_size {
        concurrency / 2
    } else {
        concurrency
    };

    let single_chunk_max_size = total_size / concurrency;
    // split chunks
    let mut chunk_ranges: Vec<Range<usize>> = Vec::new();
    let mut counter: usize = 0;
    loop {
        let chunk_start = counter * single_chunk_max_size;
        let chunk_end = (counter + 1) * single_chunk_max_size - 1;

        // chunk the border
        let chunk_end = if total_size > chunk_end {
            chunk_end
        } else {
            // EOF
            total_size
        };

        // add chunk
        chunk_ranges.push(chunk_start..chunk_end);

        if chunk_end == total_size {
            // the last chunk
            break;
        }
        counter += 1;
    }

    let client = Arc::new(client.clone());

    // order, file
    let (tx, mut rx) = mpsc::channel(20);

    // start download tasks
    for (chunk_num, chunk_range) in chunk_ranges.into_iter().enumerate() {
        let client = Arc::clone(&client);
        let url = url;
        let url = url.to_string();
        let tx = tx.clone();
        tokio::spawn(async move {
            for _retry_count in 1..max_retries {
                let result: anyhow::Result<()> = async {
                    // create temp file
                    let (mut chunk_file_handle, chunk_file_path) =
                        tempfile_async::tempfile().await?;

                    let range = format!("bytes={}-{}", chunk_range.start, chunk_range.end);
                    // download chunk
                    let mut stream = client
                        .get(&url)
                        .header("Range", range)
                        .send()
                        .await?
                        .bytes_stream();

                    // write stream to chunk_file
                    while let Some(chunk) = stream.next().await {
                        let chunk = chunk?;
                        chunk_file_handle.write_all(&chunk).await?;
                    }

                    // now this chunk is download successfully
                    // add chunk_file_handle and path to completed files (with order)
                    tx.send((chunk_num, chunk_file_handle, chunk_file_path))
                        .await?;
                    Ok(())
                }
                .await;

                if let Ok(()) = result {
                    // download successfully
                    break;
                }
            }
        });
    }

    let mut completed_tasks = Vec::new();

    while let Some(completed_task) = rx.recv().await {
        // add to vec
        completed_tasks.push(completed_task);
    }

    // sort completed tasks
    completed_tasks.sort_by(|task_a, task_b| task_a.0.cmp(&task_b.0));

    let mut hasher = expected_file_hash.map(|hash| hash.create_hasher());

    // join chunks
    for (_, chunk_file, chunk_path) in completed_tasks.into_iter() {
        let mut reader = BufReader::new(chunk_file);

        stream_write_and_calculate_hash(&mut reader, file, &mut hasher.as_mut()).await?;
        // This chunk was dumped into the target file
        // so delete it
        fs::remove_file(chunk_path).await?;
    }

    // verify hash
    if let Some(hasher) = hasher {
        let actual_hash = hex::encode(hasher.finalize());
        let expected_hash = expected_file_hash.unwrap();
        if actual_hash != expected_hash.value() {
            return Err(DownloadError::Hashing(HashingError::HashNotMatch {
                expected_hash: expected_hash.to_owned(),
                actual_hash: actual_hash,
            }));
        }
    }

    Ok(())
}

pub async fn download_single_thread(
    client: &Client,
    url: &str,
    file: &mut File,
    file_hash: Option<&Hash>,
    max_retries: u32,
) -> Result<(), DownloadError> {
    for retry_count in 1..=max_retries {
        // get file
        let result: anyhow::Result<()> = {
            let mut stream = client.get(url).send().await?.bytes_stream();

            let mut hasher = file_hash.map(|hash| hash.create_hasher());
            // stream write file
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                file.write_all(&chunk).await?;

                // update hasher if possible
                hasher
                    .iter_mut()
                    .next()
                    .map(|hasher| hasher.update(&chunk))
                    .unwrap_or(());
            }
            // check hash
            if let Some(file_hash) = file_hash {
                // compare hash
                let hasher = hasher.unwrap();
                let actual_hash = hex::encode(hasher.finalize());
                if file_hash.value() != actual_hash {
                    return Err(DownloadError::Hashing(HashingError::HashNotMatch {
                        expected_hash: file_hash.to_owned(),
                        actual_hash,
                    }));
                }
            }
            Ok(())
        };

        if let Err(err) = result {
            error!(
                "Error happened when download file (retry {retry_count}/{max_retries}): {err}, \n{}",
                Backtrace::capture()
            );
        } else {
            // operation success
            return Ok(());
        }
    }
    error!(
        "Failed to download file with hash {}: Max retries exceeded",
        file_hash
            .map(|hash| hash.value())
            .unwrap_or("<unknown hash>")
    );
    Err(DownloadError::MaxRetriesExceeded {
        url: url.to_string(),
        max_retries,
    })
}
