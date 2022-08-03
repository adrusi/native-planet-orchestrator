#[allow(unused_imports)] use crate::prelude::*;

use async_std::fs;
use async_std::io;
use async_std::path::{Path, PathBuf};
use libarchive::archive::ExtractOption;
use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error as StdError;
use std::fmt::Display;
use std::ops::Range;
use tokio::process;

use crate::archive;
use crate::filelock::FileLock;
use crate::net_util::TcpPortIssuer;
use crate::runtime;

pub use harbor_private::{HARBOR, Harbor, HarborBuf};

mod harbor_private {
    #[allow(unused_imports)] use crate::prelude::*;

    use async_std::fs::DirEntry;
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

        pub async fn port_path(&self) -> Result<PathBuf> {
            let mut result = self.0.to_owned();
            result.push(Path::new("port"));

            if !result.is_dir().await {
                bail!("Harbor port path is not a directory: {}", result.to_string_lossy())
            }

            Ok(result)
        }

        pub async fn dry_dock_path(&self) -> Result<PathBuf> {
            let mut result = self.0.to_owned();
            result.push(Path::new("dry_dock"));

            if !result.is_dir().await {
                bail!("Harbor dry dock path is not a directory: {}", result.to_string_lossy())
            }

            Ok(result)
        }

        pub fn as_path(&self) -> &Path {
            self.into()
        }

        pub async fn piers_in_port(&self) -> Result<Vec<String>> {
            let directory_listing = self.port_path().await?.read_dir().await?;

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

lazy_static! {
    pub static ref HTTP_PORT_RANGE: Range<u16> = env::var_os("NUCLEUS_HTTP_PORT_RANGE")
        .map(|s| s.to_str().unwrap().parse::<MyRange<u16>>().unwrap().inner)
        .unwrap_or(8300..8400);

    pub static ref AMES_PORT_RANGE: Range<u16> = env::var_os("NUCLEUS_AMES_PORT_RANGE")
        .map(|s| s.to_str().unwrap().parse::<MyRange<u16>>().unwrap().inner)
        .unwrap_or(4300..4400);
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

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PierConfig {
    runtime_version: runtime::Version,
    id: Uuid,
    #[serde(rename = "@p")]
    name: Option<String>,
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
    id: Uuid,
    name: Option<String>,
    config: PierConfig,
    meta_path: PathBuf,
    dry_docked: bool,
    /// true iff there's a "pier" directory
    initialized: bool,
    /// false if initialized, used to indicate whether to perform the initial launch with a keyfile or as a comet
    comet: bool,
    filelock: FileLock,
}

impl PierState {
    async fn load_from_port(path: &Path, name: &str) -> Result<Self> {
        let mut meta_path = HARBOR.port_path().await?;
        meta_path.push(name);

        if !meta_path.is_dir().await {
            bail!("Pier '{}' does not exist in harbor port", name);
        }

        let filelock = FileLock::try_acquire(
            Self::lockfile_path_given_meta(meta_path.clone())
        ).await?;
        let filelock = filelock.ok_or_else(|| anyhow!(
            "Attempted to acquire multiple handles for the same pier: {}",
            meta_path.to_string_lossy(),
        ))?;

        let config = Self::load_config(&meta_path).await?;

        match config.name {
            None => {
                bail!("attempted to load uninitialized pier from port; only dry dock piers may be uninitialized")
            },
            Some(ref config_name) => {
                if config_name != name {
                    bail!("mismatch between name of pier directory and the @p field in its config");
                }
            },
        }

        let config = Self::load_config(&meta_path).await?;

        let result = Self {
            id: config.id,
            name: Some(name.to_owned()),
            meta_path,
            filelock,
            config,
            dry_docked: false,
            comet: false,
            initialized: true,
        };

        if !result.pier_path().exists().await {
            bail!("attempted to load uninitialized pier from port; only dry dock piers may be uninitialized")
        }

        Ok(result)
    }

    async fn load_from_dry_dock(path: &Path, id: Uuid) -> Result<Self> {
        let mut meta_path = HARBOR.dry_dock_path().await?;
        meta_path.push(format!("{}", id.hyphenated()));

        if !meta_path.is_dir().await {
            bail!("Pier '{}' does not exist in harbor dry dock", id.hyphenated());
        }

        let filelock = FileLock::try_acquire(
            Self::lockfile_path_given_meta(meta_path.clone())
        ).await?;
        let filelock = filelock.ok_or_else(|| anyhow!(
            "Attempted to acquire multiple handles for the same pier: {}",
            meta_path.to_string_lossy(),
        ))?;

        let config = Self::load_config(&meta_path).await?;

        if config.id != id {
            bail!("mismatch between id of pier directory and the id field in its config");
        }

        let mut result = Self {
            id: id,
            name: config.name.clone(),
            meta_path,
            filelock,
            config: config,
            dry_docked: true,
            comet: false,
            initialized: false,
        };

        result.initialized = result.pier_path().exists().await;

        Ok(result)
    }

    async fn load_config(meta_path: &Path) -> Result<PierConfig> {
        let config_buf = fs::read(Self::config_path_given_meta(meta_path.to_owned())).await?;
        Ok(serde_json::from_slice(&config_buf)?)
    }

    pub async fn new_from_keyfile<In: io::Read + Unpin>(
        key_infile: &mut In,
        name: String,
    ) -> Result<Self> {
        let id = Uuid::new_v4();

        let mut meta_path = HARBOR.dry_dock_path().await?;
        meta_path.push(format!("{}", id.hyphenated()));

        fs::create_dir(&meta_path).await?;

        let filelock = FileLock::try_acquire(
            Self::lockfile_path_given_meta(meta_path.clone())
        ).await?;
        let filelock = filelock.ok_or_else(|| anyhow!("failed to acquire lock on newly created pier"))?;

        let config = PierConfig {
            id: id,
            name: Some(name.clone()),
            runtime_version: runtime::Version::default(),
        };

        let result = Self {
            id,
            name: Some(name),
            filelock,
            config,
            meta_path,
            dry_docked: true,
            comet: false,
            initialized: false,
        };

        let mut key_outfile = fs::OpenOptions::new()
            .read(false)
            .write(true)
            .truncate(true)
            .create_new(true)
            .open(result.keyfile_path())
            .await?;
        io::copy(key_infile, &mut key_outfile).await?;

        Ok(result)
    }

    pub async fn new_from_pier_archive<In>(
        archive_infile: &mut In,
    ) -> Result<Self>
        where In: io::Read + Unpin
    {
        let id = Uuid::new_v4();

        let mut meta_path = HARBOR.dry_dock_path().await?;
        meta_path.push(format!("{}", id.hyphenated()));

        fs::create_dir(&meta_path).await?;

        let filelock = FileLock::try_acquire(
            Self::lockfile_path_given_meta(meta_path.clone())
        ).await?;
        let filelock = filelock.ok_or_else(|| anyhow!("failed to acquire lock on newly created pier"))?;

        let config = PierConfig {
            id: id,
            name: None,
            runtime_version: runtime::Version::default(),
        };

        let result = Self {
            id,
            name: None,
            filelock,
            config,
            meta_path,
            dry_docked: true,
            comet: false,
            initialized: false,
        };

        let archive_path = result.archive_path();
        let unpack_path = result.unpack_path();
        let mut result = Self::new_from_pier_archive_inner(archive_infile, result, &archive_path, &unpack_path).await?;

        if archive_path.is_file().await {
            _ = fs::remove_file(&archive_path).await;
        }
        if unpack_path.is_dir().await {
            _ = fs::remove_dir_all(&unpack_path).await;
        }

        result.initialized = true;

        Ok(result)
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
        fs::create_dir(&result.meta_path).await?;

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

    pub async fn new_comet(
        config: Option<PierConfig>,
    ) -> Result<Self> {
        let id = Uuid::new_v4();

        let mut meta_path = HARBOR.dry_dock_path().await?;
        meta_path.push(format!("{}", id.hyphenated()));

        fs::create_dir(&meta_path).await?;

        let filelock = FileLock::try_acquire(
            Self::lockfile_path_given_meta(meta_path.clone())
        ).await?;
        let filelock = filelock.ok_or_else(|| anyhow!("failed to acquire lock on newly created pier"))?;

        let config = PierConfig {
            id: id,
            name: None,
            runtime_version: runtime::Version::default(),
        };

        let result = Self {
            id,
            name: None,
            filelock,
            config,
            meta_path,
            dry_docked: true,
            comet: true,
            initialized: false,
        };

        Ok(result)
    }

    pub fn config(&self) -> &PierConfig {
        &self.config
    }

    pub fn dry_docked(&self) -> bool {
        self.dry_docked
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn initialized(&self) -> bool {
        self.initialized
    }

    fn config_path_given_meta(mut meta_path: PathBuf) -> PathBuf {
        meta_path.push("config.json");
        meta_path
    }

    fn lockfile_path_given_meta(mut meta_path: PathBuf) -> PathBuf {
        meta_path.push("lockfile");
        meta_path
    }

    fn pier_path(&self) -> PathBuf {
        self.meta_path.join("pier")
    }

    fn keyfile_path(&self) -> PathBuf {
        self.meta_path.join("keyfile")
    }

    fn archive_path(&self) -> PathBuf {
        self.meta_path.join("archive")
    }

    fn unpack_path(&self) -> PathBuf {
        self.meta_path.join("unpack")
    }

    pub async fn release_from_dry_dock(
        mut self,
        http_port_issuer: &mut TcpPortIssuer,
        ames_port_issuer: &mut TcpPortIssuer,
    ) -> Result<Self> {
        let mut ship = self.launch(http_port_issuer, ames_port_issuer).await?;
        ship.pier.name = Some(ship.dojo("our").await?.trim().to_owned());
        self = ship.shutdown().await?;

        let mut new_meta_path = HARBOR.port_path().await?;
        new_meta_path.push(self.name.as_ref().unwrap());

        let old_meta_path = self.meta_path.clone();
        self.meta_path = new_meta_path;

        fs::rename(&old_meta_path, &self.meta_path).await?;
        self.dry_docked = false;

        Ok(self)
    }

    pub async fn launch(
        mut self,
        http_port_issuer: &mut TcpPortIssuer,
        ames_port_issuer: &mut TcpPortIssuer,
    ) -> Result<Ship> {

        let ames_port = ames_port_issuer.get_port().await?;
        let http_port = http_port_issuer.get_port().await?;

        let proc = if self.initialized {
            self.config.runtime_version.exec(
                runtime::Options::launch_existing_pier(&self.pier_path())
                    .http_port(http_port)
                    .ames_port(ames_port)
            ).await?
        } else {
            if self.comet {
                self.config.runtime_version.exec(
                    runtime::Options::launch_new_comet(&self.pier_path())
                        .http_port(http_port)
                        .ames_port(ames_port)
                ).await?
            } else {
                let name = self.name.as_ref().unwrap();
                self.config.runtime_version.exec(
                    runtime::Options::launch_from_keyfile(&self.keyfile_path(), name, &self.pier_path())
                        .http_port(http_port)
                        .ames_port(ames_port)
                ).await?
            }
        };

        self.initialized = true;

        Ok(Ship::new(self, proc, ames_port, http_port).await?)
    }
}

impl Drop for PierState {
    fn drop(&mut self) {
        let config_file = std::fs::OpenOptions::new()
            .create(true)
            .read(false)
            .write(true)
            .truncate(true)
            .open(PierState::config_path_given_meta(self.meta_path.clone()));
        let config_file = match config_file {
            Ok(f) => f,
            Err(err) => {
                log::error!("encountered error during PierState cleanup: {}", err);
                return
            },
        };

        _ = serde_json::to_writer(config_file, &self.config).unwrap_or_else(|err| {
            log::error!("encountered error during PierState cleanup: {}", err);
        });
    }
}

pub struct Ship {
    pier: PierState,
    proc: process::Child,
    http_port: u16,
    ames_port: u16,
    lens_port: u16,
}

impl Ship {
    async fn new(pier: PierState, proc: process::Child, http_port: u16, ames_port: u16) -> Result<Self> {
        let portsfile_path = pier.pier_path().join(&Path::new(".http.ports"));
        let portsdesc = fs::read_to_string(&portsfile_path).await?;

        let lens_port: u16 = portsdesc.lines()
            .filter(|line| line.ends_with("loopback"))
            .map(|line| line.split_ascii_whitespace().nth(0))
            .nth(0)
            .flatten()
            .and_then(|port_str| port_str.parse().ok())
            .ok_or(anyhow!("could not decode .http.ports file: {}", portsfile_path.to_string_lossy()))?;

        Ok(Ship {
            pier, proc, http_port, ames_port,
            lens_port,
        })
    }

    pub async fn shutdown(mut self) -> Result<PierState> {
        self.proc.kill().await?;
        Ok(self.pier)
    }

    pub async fn dojo(&self, eval_str: &str) -> Result<String> {
        let res_json = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}", self.lens_port))
            .header("Content-type", "application/json")
            .json(&serde_json::json!({
                "source": { "dojo": eval_str },
                "sink": { "stdout": null },
            }))
            .send()
            .await?
            .bytes()
            .await?;

        let res_json: serde_json::Value = serde_json::from_slice(&res_json)?;

        match res_json {
            serde_json::Value::String(s) => Ok(s),
            _ => bail!("invalid response from urbit"),
        }
    }
}