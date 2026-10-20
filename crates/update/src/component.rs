//! Components Birchpad installs on request, apart from itself (ADR 0045): PlantUML with the Java
//! that runs it, so that PlantUML previews work without installing either. Each is a ZIP of a
//! release of Birchpad's repository, pinned here by its size and SHA-256 for each platform, so
//! that what is downloaded is what Birchpad's own release workflow built. It is downloaded only
//! when the user asks, through the transport of updates (its proxy too), checked, and unpacked
//! into Birchpad's data folder.

use std::fs::{self, File};
use std::io::Read as _;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use crate::{Transport, hex};

/// A component's package for one platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentPackage {
    /// `windows-x64`, `windows-arm64`, `linux-x64`, `macos-arm64`, `macos-x64`.
    pub platform: &'static str,
    pub url: &'static str,
    pub size: u64,
    /// Lowercase hex.
    pub sha256: &'static str,
}

/// A component and its packages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Component {
    /// Its folder's name in the data folder, with [`Self::version`].
    pub name: &'static str,
    /// Its version and the package's revision (`1.2026.9-1`).
    pub version: &'static str,
    pub packages: &'static [ComponentPackage],
}

/// A package of the PlantUML release: its URL from its platform.
macro_rules! plantuml {
    ($platform:literal, $size:expr, $sha256:literal) => {
        ComponentPackage {
            platform: $platform,
            url: concat!(
                "https://github.com/SecurityTrip/Birchpad/releases/download/plantuml-1.2026.9-1/",
                "birchpad-plantuml-1.2026.9-1-",
                $platform,
                ".zip"
            ),
            size: $size,
            sha256: $sha256,
        }
    };
}

/// PlantUML (its MIT build) and a Java runtime made for it with `jlink` (Eclipse Temurin), built
/// by `.github/workflows/plantuml.yml` (the release `plantuml-1.2026.9-1`).
pub const PLANTUML: Component = Component {
    name: "plantuml",
    version: "1.2026.9-1",
    packages: &[
        plantuml!(
            "windows-x64",
            49360935,
            "8e1701ba9d14b1a66ee3eae2892752295ea78825b611c741638aab188dd654c6"
        ),
        plantuml!(
            "windows-arm64",
            47513164,
            "11b8e46cb9d4febe3ba952149f9b3503085648f5f76931bde2f0870b0b1696bc"
        ),
        plantuml!(
            "linux-x64",
            53301943,
            "6de6243862f5d7f3cf239eb11573add3d5bfb0b26700ed5332d991b2e8fd3ead"
        ),
        plantuml!(
            "macos-arm64",
            49257738,
            "9b4e9e8d47c6dd18cde4ce23c3391386c2cb5533dbaffdd20e8e6172a2cda466"
        ),
        plantuml!(
            "macos-x64",
            50428512,
            "a2c04e8a3d8b676a42a72346199d52d59c4b7249f90e15f12563eaf8f5cb2767"
        ),
    ],
};

/// The platform of this build, as components name it; `None` where there is no package.
pub const fn platform() -> Option<&'static str> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Some("windows-x64")
    } else if cfg!(all(windows, target_arch = "aarch64")) {
        Some("windows-arm64")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("linux-x64")
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("macos-arm64")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some("macos-x64")
    } else {
        None
    }
}

/// The file an installed component's folder has once it is whole.
const INSTALLED: &str = ".installed";

/// Unpacked components larger than this are refused: a package that inflates without end.
const MAX_UNPACKED: u64 = 1 << 30;

impl Component {
    /// The package for `platform`, if it has one that was published.
    pub fn package(&self, platform: &str) -> Option<&'static ComponentPackage> {
        self.packages
            .iter()
            .find(|package| package.platform == platform && package.size > 0)
    }

    /// Where it is installed in the data folder `data`.
    pub fn folder(&self, data: &Path) -> PathBuf {
        data.join("components")
            .join(format!("{}-{}", self.name, self.version))
    }

    /// Its folder in `data`, if it is installed whole.
    pub fn installed(&self, data: &Path) -> Option<PathBuf> {
        let folder = self.folder(data);
        folder.join(INSTALLED).is_file().then_some(folder)
    }

    /// Downloads the package of `platform` through `transport`, checks its size and SHA-256,
    /// and unpacks it into `data`, in place of an earlier version. `progress` is told the bytes
    /// received. Returns the component's folder.
    pub fn install(
        &self,
        transport: &dyn Transport,
        data: &Path,
        platform: &str,
        progress: &dyn Fn(u64),
    ) -> Result<PathBuf, String> {
        let package = self.package(platform).ok_or_else(|| {
            birchpad_i18n::t!(
                "update.no-component-package",
                component = self.name,
                platform = platform
            )
        })?;
        install_package(self, package, transport, data, progress)
    }
}

/// [`Component::install`] for `package`, which tests make their own.
pub fn install_package(
    component: &Component,
    package: &ComponentPackage,
    transport: &dyn Transport,
    data: &Path,
    progress: &dyn Fn(u64),
) -> Result<PathBuf, String> {
    let components = data.join("components");
    fs::create_dir_all(&components).map_err(|error| error.to_string())?;
    let name = format!("{}-{}", component.name, component.version);
    let archive = components.join(format!("{name}.zip.part"));
    let unpacking = components.join(format!("{name}.part"));
    let cleanup = || {
        fs::remove_file(&archive).ok();
        fs::remove_dir_all(&unpacking).ok();
    };
    cleanup();
    let result = transport
        .download(package.url, &archive, package.size, progress)
        .map_err(|reason| {
            birchpad_i18n::t!("update.cannot-download", url = package.url, reason = reason)
        })
        .and_then(|()| verify(&archive, package))
        .and_then(|()| unpack(&archive, &unpacking));
    if let Err(error) = result {
        cleanup();
        return Err(error);
    }
    fs::remove_file(&archive).ok();
    let folder = component.folder(data);
    fs::remove_dir_all(&folder).ok();
    fs::File::create(unpacking.join(INSTALLED)).map_err(|error| error.to_string())?;
    fs::rename(&unpacking, &folder).map_err(|error| {
        cleanup();
        error.to_string()
    })?;
    // Earlier versions of the component go.
    if let Ok(entries) = fs::read_dir(&components) {
        let prefix = format!("{}-", component.name);
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if file_name.starts_with(&prefix) && file_name != name && entry.path().is_dir() {
                fs::remove_dir_all(entry.path()).ok();
            }
        }
    }
    Ok(folder)
}

/// Checks that the file at `path` has the package's size and SHA-256.
fn verify(path: &Path, package: &ComponentPackage) -> Result<(), String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    let mut size = 0;
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        size += read as u64;
        hasher.update(&buffer[..read]);
    }
    let sha256 = hex(&hasher.finalize());
    if size != package.size || !sha256.eq_ignore_ascii_case(package.sha256) {
        return Err(birchpad_i18n::t!(
            "update.component-not-as-built",
            url = package.url
        ));
    }
    Ok(())
}

/// Unpacks the ZIP at `archive` into `to`; names that leave `to` are refused.
fn unpack(archive: &Path, to: &Path) -> Result<(), String> {
    let file = File::open(archive).map_err(|error| error.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
    let mut unpacked = 0u64;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|error| error.to_string())?;
        let Some(name) = entry.enclosed_name() else {
            return Err(format!("{} is outside the package", entry.name()));
        };
        let path = to.join(name);
        if entry.is_dir() {
            fs::create_dir_all(&path).map_err(|error| error.to_string())?;
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut out = File::create(&path).map_err(|error| error.to_string())?;
        let limit = MAX_UNPACKED - unpacked;
        let written = std::io::copy(&mut (&mut entry).take(limit + 1), &mut out)
            .map_err(|error| error.to_string())?;
        unpacked += written;
        if unpacked > MAX_UNPACKED {
            return Err("the package unpacks to more than 1 GB".to_owned());
        }
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&path, fs::Permissions::from_mode(mode & 0o755))
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}
