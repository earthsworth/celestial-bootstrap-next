use bytes::Bytes;
use digest::{Digest, OutputSizeUser, generic_array::ArrayLength};
use serde::{Deserialize, Serialize};
use std::{
    ops::Add,
    path::{Path, PathBuf},
};
use thiserror::Error;
use tokio::io::{AsyncReadExt, BufReader};

#[derive(Error, Debug)]
pub enum HashingError {
    #[error("Hash mismatch: expected {expected_hash:?}, got {actual_hash}")]
    HashNotMatch {
        expected_hash: Hash,
        actual_hash: String,
    },
    #[error("Hash mismatch for file {file_path}: expected {expected_hash:?}, got {actual_hash}")]
    FileHashNotMatch {
        file_path: Box<PathBuf>,
        expected_hash: Hash,
        actual_hash: String,
    },

    #[error("File does not exist: {file_path}")]
    FileNotFound { file_path: Box<PathBuf> },

    #[error("Io Error")]
    IoError(#[from] std::io::Error),

    #[error("Unsupported hash type: {0}")]
    UnsupportedHashFunction(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Hash {
    Md5(String),
    Sha1(String),
    Sha256(String),
    Sha512(String),
}

impl Hash {
    pub fn hash_type(&self) -> &'static str {
        match self {
            Hash::Md5(_) => "MD5",
            Hash::Sha1(_) => "SHA1",
            Hash::Sha256(_) => "SHA256",
            Hash::Sha512(_) => "SHA512",
        }
    }

    pub fn value(&self) -> &str {
        match self {
            Hash::Md5(v) | Hash::Sha1(v) | Hash::Sha256(v) | Hash::Sha512(v) => v,
        }
    }

    pub fn create_hasher(&self) -> Box<dyn digest::DynDigest> {
        match self {
            Hash::Md5(_) => Box::new(md5::Md5::new()),
            Hash::Sha1(_) => Box::new(sha1::Sha1::new()),
            Hash::Sha256(_) => Box::new(sha2::Sha256::new()),
            Hash::Sha512(_) => Box::new(sha2::Sha512::new()),
        }
    }
}

/// Asynchronously streams a file and calculates its hash using a generic hasher.
///
/// This function reads the file in chunks to keep memory usage low, making it
/// suitable for large files.
///
/// # Arguments
/// * `file_path`: The path to the file to hash.
///
/// # Type Parameters
/// * `D`: The hash algorithm to use, which must implement the `digest::Digest` trait.
///
/// # Returns
/// A `Result` containing the lowercase hex-encoded hash string, or a `HashingError`.
async fn stream_and_hash<D: Digest>(file_path: &Path) -> Result<String, HashingError>
where
    <D as OutputSizeUser>::OutputSize: Add,
    <<D as OutputSizeUser>::OutputSize as Add>::Output: ArrayLength<u8>,
{
    if !file_path.exists() {
        return Err(HashingError::FileNotFound {
            file_path: Box::new(file_path.to_owned()),
        });
    }

    let file = tokio::fs::File::open(file_path)
        .await
        .map_err(HashingError::from)?;

    let mut reader = BufReader::new(file);
    let mut hasher = D::new();
    let mut buffer = [0; 8192]; // 8KB buffer

    loop {
        let bytes_read = reader.read(&mut buffer).await.map_err(HashingError::from)?;
        if bytes_read == 0 {
            // EOF
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

/// Compare file hash with expected hash.
/// The file is read in a stream to support large files without high memory consumption.
/// Returns Ok(()) if hash matches, Err if file doesn't exist or hash doesn't match.
pub async fn compare_file_hash(file_path: &Path, expected_hash: &Hash) -> Result<(), HashingError> {
    let calculated_hash = calculate_file_hash(file_path, expected_hash.hash_type()).await?;
    let calculated_hash = calculated_hash.value();

    if calculated_hash.eq_ignore_ascii_case(expected_hash.value()) {
        Ok(())
    } else {
        Err(HashingError::FileHashNotMatch {
            file_path: Box::new(file_path.to_owned()),
            expected_hash: expected_hash.to_owned(),
            actual_hash: calculated_hash.to_string(),
        })
    }
}

/// Calculate hash for a file.
/// The file is read in a stream to support large files without high memory consumption.
pub async fn calculate_file_hash(file_path: &Path, hash_type: &str) -> Result<Hash, HashingError> {
    use md5::Md5;
    use sha1::Sha1;
    use sha2::{Sha256, Sha512};

    let hash = match hash_type.to_uppercase().as_str() {
        "MD5" => Hash::Md5(stream_and_hash::<Md5>(file_path).await?),
        "SHA1" => Hash::Sha1(stream_and_hash::<Sha1>(file_path).await?),
        "SHA256" => Hash::Sha256(stream_and_hash::<Sha256>(file_path).await?),
        "SHA512" => Hash::Sha512(stream_and_hash::<Sha512>(file_path).await?),
        _ => return Err(HashingError::UnsupportedHashFunction(hash_type.to_string())),
    };

    Ok(hash)
}
/// Compare bytes hash with expected hash
/// Returns Ok(()) if hash matches, Err if file doesn't exist or hash doesn't match
pub async fn compare_hash(bytes: &Bytes, expected_hash: &Hash) -> Result<(), HashingError> {
    // Calculate hash based on expected hash type
    let calculated_hash = match expected_hash {
        Hash::Md5(_) => {
            use md5::{Digest, Md5};
            let mut hasher = Md5::new();
            hasher.update(bytes);
            format!("{:x}", hasher.finalize())
        }
        Hash::Sha1(_) => {
            use sha1::{Digest, Sha1};
            let mut hasher = Sha1::new();
            hasher.update(bytes);
            format!("{:x}", hasher.finalize())
        }
        Hash::Sha256(_) => {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(bytes);
            format!("{:x}", hasher.finalize())
        }
        Hash::Sha512(_) => {
            use sha2::{Digest, Sha512};
            let mut hasher = Sha512::new();
            hasher.update(bytes);
            format!("{:x}", hasher.finalize())
        }
    };

    // Compare hashes (case-insensitive)
    if calculated_hash.to_lowercase() == expected_hash.value().to_lowercase() {
        Ok(())
    } else {
        Err(HashingError::HashNotMatch {
            expected_hash: expected_hash.to_owned(),
            actual_hash: calculated_hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_compare_hash_success() {
        // Create a temporary file
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"hello world").unwrap();

        // Calculate expected hash
        let expected_hash = Hash::Sha256(
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9".to_string(),
        );

        // Test comparison
        let result = compare_file_hash(temp_file.path(), &expected_hash).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_compare_hash_mismatch() {
        // Create a temporary file
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"hello world").unwrap();

        // Wrong hash
        let wrong_hash = Hash::Sha256("wrong_hash".to_string());

        // Test comparison
        let result = compare_file_hash(temp_file.path(), &wrong_hash).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_calculate_file_hash() {
        // Create a temporary file
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"hello world").unwrap();

        // Calculate hash
        let hash = calculate_file_hash(temp_file.path(), "SHA256")
            .await
            .unwrap();

        match hash {
            Hash::Sha256(value) => {
                assert_eq!(
                    value,
                    "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
                );
            }
            _ => panic!("Expected SHA256 hash"),
        }
    }
}
