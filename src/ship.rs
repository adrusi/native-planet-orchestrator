#[allow(unused_imports)] use crate::prelude::*;

use async_std::fs;
use async_std::io;
use async_std::path::{Path, PathBuf};
use libarchive::archive::ExtractOption;
use serde::{Deserialize, Serialize};
use std::error::Error as StdError;
use std::fmt::Display;

use crate::archive;
use crate::filelock::FileLock;
use crate::urbit::Version;

pub use harbor_private::{HARBOR, Harbor, HarborBuf};

mod harbor_private {
    use anyhow::Result;
    use async_std::fs::DirEntry;
    use futures::prelude::*;
    use lazy_static::lazy_static;
    use std::borrow::Borrow;
    use std::env;
    use std::io;
    use std::ops::Deref;
    use async_std::path::{Path, PathBuf};

    lazy_static! {
        pub static ref HARBOR: HarborBuf = HarborBuf::default();
    }

    #[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
    pub struct Harbor(Path);
    #[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
    pub struct HarborBuf(PathBuf);

    impl Harbor {
        fn new<P: AsRef<Path> + ?Sized>(path: &P) -> &Self {
            unsafe { &*(path.as_ref() as *const Path as *const Harbor) }
        }

        pub async fn port_path(&self) -> PathBuf {
            let mut result = self.0.to_owned();
            result.push(Path::new("port"));

            if !result.is_dir().await {
                panic!("Harbor port path is not a directory: {}", result.to_string_lossy())
            }

            result
        }

        pub async fn dry_dock_path(&self) -> PathBuf {
            let mut result = self.0.to_owned();
            result.push(Path::new("dry_dock"));

            if !result.is_dir().await {
                panic!("Harbor dry dock path is not a directory: {}", result.to_string_lossy())
            }

            result
        }

        pub fn as_path(&self) -> &Path {
            self.into()
        }

        pub async fn piers_in_port(&self) -> Result<Vec<String>> {
            let directory_listing = self.port_path().await.read_dir().await?;

            let mut result: Vec<String> = Vec::new();

            for entry in directory_listing.collect::<Vec<io::Result<DirEntry>>>().await {
                let entry = entry?;
                if !entry.file_type().await?.is_dir() {
                    continue
                }
                let name = match entry.file_name().into_string() {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                result.push(name);
            }

            return Ok(result)
        }
    }

    impl<'a> Into<&'a Path> for &'a Harbor {
        fn into(self) -> &'a Path {
            &self.0
        }
    }

    impl ToOwned for Harbor {
        type Owned = HarborBuf;

        fn to_owned(&self) -> HarborBuf {
            HarborBuf(self.0.to_owned())
        }
    }

    impl HarborBuf {
        pub fn into_boxed_harbor(self) -> Box<Harbor> {
            let rw = Box::into_raw(self.0.into_boxed_path()) as *mut Harbor;
            unsafe { Box::from_raw(rw) }
        }
    }

    impl Deref for HarborBuf {
        type Target = Harbor;

        fn deref(&self) -> &Harbor {
            Harbor::new(&self.0)
        }
    }

    impl Borrow<Harbor> for HarborBuf {
        fn borrow(&self) -> &Harbor {
            self.deref()
        }
    }

    impl<'a> From<&'a Harbor> for HarborBuf {
        fn from(h: &'a Harbor) -> Self {
            h.to_owned()
        }
    }

    impl Default for HarborBuf {
        fn default() -> Self {
            use std::path::{Path, PathBuf};

            let path = env::var_os("NUCLEUS_HARBOR_PATH")
                .map(|p| PathBuf::from(p))
                .unwrap_or(
                    Path::new("/var/harbor").to_owned()
                );

            if !path.is_dir() {
                panic!("Harbor path is not a directory: {}", path.to_string_lossy())
            }

            HarborBuf(path.into())
        }
    }
}

#[derive(Debug)]
pub struct InvalidPierArchiveError;

impl Display for InvalidPierArchiveError {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl StdError for InvalidPierArchiveError {}

fn find_extracted_pier(_unpack_path: &Path) -> Option<PathBuf> {
    todo!();
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct PierConfig {
    runtime_version: Version,
}

/// A PierState represents the data for an Urbit ship. Specifically it is a unique handle to the directory where all
/// data for the particular ship is stored.
/// The type system guarantees that there cannot be multiple PierState handles for the same directory, in order to
/// prevent accidentally corrupting valuable user data.
///
/// IMPORTANT: before letting a PierState go out of scope and be dropped, you must call `pier.async_drop().await`. The
/// pier must do filesystem IO to release a lock, and async IO isn't possible with std::ops::Drop. The lock will still
/// be released if you forget, but synchronously, blocking the whole thread and tanking performance.
#[derive(Debug)]
pub struct PierState {
    filelock: Option<FileLock>,
    config: Option<PierConfig>,
    meta_path: PathBuf,
    dry_docked: bool,
    name: String,
    initialized: bool,
    running: bool,
}

impl PierState {
    async fn prepare_for_load(mut path: PathBuf, name: &str) -> Result<Self> {
        path.push(name);

        if !path.is_dir().await {
            bail!(
                "Pier '{}' does not exist in harbor port: {}",
                name,
                HARBOR.as_path().to_string_lossy(),
            )
        }

        let mut result = Self {
            filelock: None,
            config: None,
            meta_path: Self::determine_meta_path(name, false).await,
            dry_docked: false,
            name: name.to_owned(),
            initialized: false,
            running: false,
        };

        if result.pier_path().exists().await {
            result.initialized = true;
        }

        Ok(result)
    }

    async fn load_config(&mut self) -> Result<()> {
        let config_buf = fs::read(self.config_path()).await?;
        self.config = Some(serde_json::from_slice(&config_buf)?);
        Ok(())
    }

    pub async fn try_load_from_port(name: &str) -> Result<Self> {
        Self::try_load_from_path(HARBOR.port_path().await, name).await
    }

    pub async fn try_load_from_dry_dock(name: &str) -> Result<Self> {
        Self::try_load_from_path(HARBOR.dry_dock_path().await, name).await
    }

    async fn try_load_from_path(path: PathBuf, name: &str) -> Result<Self> {
        let mut result = PierState::prepare_for_load(path, name).await?;

        let filelock = FileLock::try_acquire(result.lockfile_path()).await?;
        if filelock.is_none() {
            bail!(
                "Attempted to acquire multiple handles for the same pier: {}",
                result.meta_path().to_string_lossy(),
            )
        }

        result.filelock = filelock;

        result.load_config();

        Ok(result)
    }

    pub async fn load_from_port(name: &str) -> Result<Self> {
        Self::load_from_path(HARBOR.port_path().await, name).await
    }

    pub async fn load_from_dry_dock(name: &str) -> Result<Self> {
        Self::load_from_path(HARBOR.dry_dock_path().await, name).await
    }

    async fn load_from_path(path: PathBuf, name: &str) -> Result<Self> {
        let mut result = PierState::prepare_for_load(path, name).await?;

        result.filelock = Some(
            FileLock::acquire(result.lockfile_path()).await?
        );

        result.load_config();

        Ok(result)
    }

    pub async fn new_from_keyfile<In: io::Read + Unpin>(
        key_infile: &mut In,
        config: Option<PierConfig>
    ) -> Result<Self> {
        let name = Uuid::new_v4().hyphenated().to_string();

        let mut result = Self {
            filelock: None,
            config: Some(config.unwrap_or_default()),
            meta_path: Self::determine_meta_path(&name, true).await,
            dry_docked: true,
            name,
            initialized: false,
            running: false,
        };

        fs::create_dir(result.meta_path()).await?;

        let mut key_outfile = fs::OpenOptions::new()
            .read(false)
            .write(true)
            .truncate(true)
            .create_new(true)
            .open(result.keyfile_path())
            .await?;
        io::copy(key_infile, &mut key_outfile).await?;

        result.filelock = FileLock::try_acquire(result.lockfile_path()).await?
            .ok_or(anyhow!("failed to acquire lock on newly created pier"))?
            .into();

        Ok(result)
    }

    pub async fn new_from_pier_archive<In>(
        archive_infile: &mut In,
        config: Option<PierConfig>,
    ) -> Result<Self>
        where In: io::Read + Unpin
    {
        let name = Uuid::new_v4().hyphenated().to_string();

        let result = Self {
            filelock: None,
            config: Some(config.unwrap_or_default()),
            meta_path: Self::determine_meta_path(&name, true).await,
            dry_docked: true,
            name,
            initialized: true,
            running: false,
        };

        let archive_path = result.archive_path();
        let unpack_path = result.unpack_path();
        let result = Self::new_from_pier_archive_inner(archive_infile, result, &archive_path, &unpack_path).await;

        if archive_path.is_file().await {
            _ = fs::remove_file(&archive_path).await;
        }
        if unpack_path.is_dir().await {
            _ = fs::remove_dir_all(&unpack_path).await;
        }

        result
    }

    // All the business logic is here, split out to allow simpler cleanup in the face of no async Drop.
    #[inline]
    async fn new_from_pier_archive_inner<In>(
        archive_infile: &mut In,
        result: Self,
        archive_path: &Path,
        unpack_path: &Path,
    ) -> Result<Self>
        where In: io::Read + Unpin
    {
        fs::create_dir(result.meta_path()).await?;

        let mut archive_outfile = fs::OpenOptions::new()
            .read(false)
            .write(true)
            .truncate(true)
            .create_new(true)
            .open(&archive_path)
            .await?;
        io::copy(archive_infile, &mut archive_outfile).await?;

        fs::create_dir(&unpack_path).await?;

        let mut extract_options = archive::safe_extract_options();
        extract_options.add(ExtractOption::Time);
        archive::extract_file(
            archive_path.to_owned(),
            unpack_path.to_owned(),
            extract_options,
        ).await?;

        fs::remove_file(&archive_path).await?;

        let extracted_pier_path = find_extracted_pier(&unpack_path).ok_or(InvalidPierArchiveError)?;
        fs::rename(&extracted_pier_path, result.pier_path()).await?;

        fs::remove_dir_all(&unpack_path).await?;

        Ok(result)
    }

    pub fn config(&self) -> Option<&PierConfig> {
        self.config.as_ref()
    }

    pub fn dry_docked(&self) -> bool {
        self.dry_docked
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn initialized(&self) -> bool {
        self.initialized
    }

    async fn determine_meta_path(name: &str, dry_docked: bool) -> PathBuf {
        let parent_path = if dry_docked {
            HARBOR.dry_dock_path().await
        } else {
            HARBOR.port_path().await
        };

        let mut result = parent_path.to_owned();
        result.push(Path::new(name));

        result
    }

    fn meta_path(&self) -> PathBuf {
        self.meta_path.clone()
    }

    fn pier_path(&self) -> PathBuf {
        let mut result = self.meta_path();
        result.push(Path::new("pier"));
        result
    }

    fn config_path(&self) -> PathBuf {
        let mut result = self.meta_path();
        result.push("config.json");
        result
    }

    fn lockfile_path(&self) -> PathBuf {
        let mut result = self.meta_path();
        result.push("lockfile");
        result
    }

    fn keyfile_path(&self) -> PathBuf {
        let mut result = self.meta_path();
        result.push("keyfile");
        result
    }

    fn archive_path(&self) -> PathBuf {
        let mut result = self.meta_path();
        result.push("archive");
        result
    }

    fn unpack_path(&self) -> PathBuf {
        let mut result = self.meta_path();
        result.push("unpack");
        result
    }

    pub async fn release_from_dry_dock(&mut self, new_name: String) -> Result<()> {
        if self.running {
            bail!(
                "cannot release running ship from dry dock: {} (new name {})",
                self.name, new_name,
            )
        }
        if !self.initialized {
            bail!(
                "cannot release uninitialized ship from dry dock: {} (new name {})",
                self.name, new_name,
            )
        }

        let src_path = self.meta_path();
        let mut dst_path = HARBOR.port_path().await;
        dst_path.push(&new_name);

        fs::rename(src_path, dst_path).await?;

        self.name = new_name;

        Ok(())
    }
}

#[async_trait]
impl AsyncDrop for PierState {
    async fn async_drop_result(&mut self) -> Result<()> {
        match self.config {
            None => {},
            Some(ref config) => {
                let config_buf = serde_json::to_vec(config)?;
                fs::write(self.config_path(), &config_buf).await?;
                self.config = None;
            },
        }

        let filelock = self.filelock.take();
        match filelock {
            None => Ok(()),
            Some(filelock) => filelock.release().await,
        }
    }
}

impl Drop for PierState {
    fn drop(&mut self) {
        match self.config {
            None => {},
            Some(ref config) => {
                log::error!(
                    "PierState not finalized before being dropped, performing sync IO in async context to clean up"
                );

                let config_file = std::fs::OpenOptions::new()
                    .create(true)
                    .read(false)
                    .write(true)
                    .truncate(true)
                    .open(self.config_path());
                let config_file = match config_file {
                    Ok(f) => f,
                    Err(err) => {
                        log::error!("encountered error during PierState cleanup: {}", err);
                        return
                    },
                };

                match serde_json::to_writer(config_file, config) {
                    Ok(_) => {},
                    Err(err) => {
                        log::error!("encountered error during PierState cleanup: {}", err);
                    }
                }
            },
        }
    }
}

pub struct Ship {
    pub pier: PierState,
}