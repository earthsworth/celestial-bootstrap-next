use digest::DynDigest;
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
};
use tokio_util::compat::TokioAsyncReadCompatExt;

use crate::utils::hashing::HashingError;

pub async fn stream_write_and_calculate_hash(
    reader: &mut (impl TokioAsyncReadCompatExt + Unpin),
    out_file: &mut fs::File,
    hasher: &mut Option<&mut Box<dyn DynDigest>>,
) -> Result<(), HashingError> {
    loop {
        let mut buffer = [0; 8192];
        let bytes_read = reader.read(&mut buffer).await.map_err(HashingError::from)?;
        if bytes_read == 0 {
            // EOF
            break Ok(());
        }
        let bytes = &buffer[..bytes_read];
        // update Hasher
        if let Some(hasher) = hasher {
            hasher.update(bytes);
        }

        // write to file
        out_file.write_all(bytes).await?;
    }
}
