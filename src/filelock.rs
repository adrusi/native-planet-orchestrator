use anyhow::Result;
use async_std::path::PathBuf;
use async_std::fs;
use log::error;

#[derive(Debug)]
pub struct FileLock {
    path: PathBuf,
    released: bool,
}

const POLL_INTERVAL_MILLIS: u64 = 50;

impl FileLock {
    pub async fn try_acquire<P: ToOwned<Owned = PathBuf>>(path: P) -> Result<Option<FileLock>> {
        let path = path.to_owned();

        if path.exists().await {
            return Ok(None);
        }

        _ = fs::File::create(&path);

        Ok(Some(FileLock {
            path: path,
            released: false,
        }))
    }

    pub async fn acquire<P: ToOwned<Owned = PathBuf>>(path: P) -> Result<FileLock> {
        let path = path.to_owned();

        while path.exists().await {
            tokio::time::sleep(tokio::time::Duration::from_millis(POLL_INTERVAL_MILLIS)).await;
        }

        _ = fs::File::create(&path);

        Ok(FileLock {
            path: path,
            released: false,
        })
    }

    pub async fn release(mut self) -> Result<()> {
        let result = fs::remove_file(&self.path).await?;
        self.released = true;
        Ok(result)
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        if !self.released {
            error!("programmer error: FileLock not released before being dropped; performing blocking IO in async context to release the lock");
            match std::fs::remove_file(&self.path) {
                Ok(_) => {},
                Err(e) => {
                    error!("error encountered while releasing file lock: {}", e);
                }
            }
        }
    }
}