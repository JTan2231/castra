use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use hex::encode as hex_encode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ureq::{Agent, Error as UreqError, ErrorKind as UreqErrorKind};

use crate::error::{Error, Result};

#[derive(Debug)]
pub struct ImageManager {
    storage_root: PathBuf,
    log_path: PathBuf,
    agent: Agent,
    qemu_img: Option<PathBuf>,
}

#[derive(Debug)]
pub struct ManagedImageEnsureOutcome {
    pub paths: ManagedImagePaths,
    pub events: Vec<ManagedArtifactEvent>,
}

#[derive(Debug, Clone)]
pub struct ManagedImagePaths {
    pub root_disk: PathBuf,
    pub kernel: Option<PathBuf>,
    pub initrd: Option<PathBuf>,
}

impl ManagedImagePaths {
    fn from_records(
        spec: &'static ManagedImageSpec,
        records: &HashMap<ManagedArtifactKind, PathBuf>,
    ) -> Result<Self> {
        let root_disk = records
            .get(&ManagedArtifactKind::RootDisk)
            .cloned()
            .ok_or_else(|| Error::PreflightFailed {
                message: format!(
                    "Managed image `{}` missing root disk after acquisition.",
                    spec.identifier()
                ),
            })?;

        let kernel = match spec.qemu.kernel {
            Some(kind) => records.get(&kind).cloned(),
            None => None,
        };
        let initrd = match spec.qemu.initrd {
            Some(kind) => records.get(&kind).cloned(),
            None => None,
        };

        Ok(Self {
            root_disk,
            kernel,
            initrd,
        })
    }
}

#[derive(Debug)]
pub struct ManagedArtifactEvent {
    pub artifact: ManagedArtifactKind,
    pub message: String,
}

impl ManagedArtifactEvent {
    fn new(artifact: ManagedArtifactKind, message: impl Into<String>) -> Self {
        Self {
            artifact,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone)]
enum CacheState {
    Missing,
    Stale(PathBuf),
}

impl ImageManager {
    pub fn new(storage_root: PathBuf, log_root: PathBuf, qemu_img: Option<PathBuf>) -> Self {
        let log_path = log_root.join("image-manager.log");
        Self {
            storage_root,
            log_path,
            agent: Agent::new(),
            qemu_img,
        }
    }

    pub fn ensure_image(
        &self,
        spec: &'static ManagedImageSpec,
    ) -> Result<ManagedImageEnsureOutcome> {
        let image_root = self.storage_root.join(spec.id).join(spec.version);
        fs::create_dir_all(&image_root).map_err(|err| Error::PreflightFailed {
            message: format!(
                "Failed to prepare managed image directory {}: {err}",
                image_root.display()
            ),
        })?;

        let manifest_path = image_root.join("manifest.json");
        let mut manifest = load_manifest(&manifest_path);
        let fingerprint = spec.fingerprint();

        if manifest.spec_digest.as_deref() != Some(&fingerprint) {
            manifest = ImageManifest::new(fingerprint.clone());
        }

        let mut events = Vec::new();
        let mut resolved_paths: HashMap<ManagedArtifactKind, PathBuf> = HashMap::new();

        for artifact in spec.artifacts {
            let final_path = image_root.join(artifact.final_filename);
            let artifact_key = artifact.final_filename.to_string();

            if let Some(record) = manifest.artifacts.get(&artifact_key) {
                if final_path.is_file() {
                    let actual = compute_sha256(&final_path)?;
                    if actual == record.final_sha256 {
                        self.push_event(
                            spec,
                            &mut events,
                            artifact.kind,
                            format!("{}: cache hit (verified).", artifact.kind.describe()),
                        );
                        resolved_paths.insert(artifact.kind, final_path.clone());
                        continue;
                    }
                }
            }

            let cache_state = if final_path.is_file() {
                CacheState::Stale(final_path.clone())
            } else {
                CacheState::Missing
            };

            self.push_event(
                spec,
                &mut events,
                artifact.kind,
                format!("{}: refreshing cached artifact.", artifact.kind.describe()),
            );

            let download_path =
                self.download_artifact(spec, &image_root, artifact, &cache_state, &mut events)?;
            if let Some(expected) = artifact.source.sha256 {
                if let Err(err) = verify_checksum(&download_path, expected) {
                    return Err(Error::PreflightFailed {
                        message: format!(
                            "Checksum mismatch for {} (expected {expected}). Remove {} and retry: {err}",
                            artifact.source.url,
                            download_path.display()
                        ),
                    });
                }
                self.push_event(
                    spec,
                    &mut events,
                    artifact.kind,
                    "verified source checksums.",
                );
            }

            let transformed_path = self.apply_transformations(
                spec,
                &image_root,
                artifact,
                download_path.clone(),
                &mut events,
            )?;

            let final_location = if transformed_path == final_path {
                transformed_path
            } else {
                fs::rename(&transformed_path, &final_path).map_err(|err| {
                    Error::PreflightFailed {
                        message: format!(
                            "Failed to place managed artifact at {}: {err}",
                            final_path.display()
                        ),
                    }
                })?;
                final_path.clone()
            };

            if download_path.exists() && download_path != final_location {
                let _ = fs::remove_file(&download_path);
            }

            let final_hash = compute_sha256(&final_location)?;
            let size = fs::metadata(&final_location)
                .map(|meta| meta.len())
                .unwrap_or_default();

            manifest.artifacts.insert(
                artifact_key,
                ManifestArtifact {
                    final_sha256: final_hash.clone(),
                    size,
                    updated_at: timestamp_seconds(),
                    source_sha256: artifact.source.sha256.map(str::to_string),
                },
            );

            self.push_event(
                spec,
                &mut events,
                artifact.kind,
                format!(
                    "{}: final checksum {} stored.",
                    artifact.kind.describe(),
                    final_hash
                ),
            );

            resolved_paths.insert(artifact.kind, final_location);
        }

        manifest.last_checked = Some(timestamp_seconds());
        save_manifest(&manifest_path, &manifest)?;

        self.push_event(
            spec,
            &mut events,
            ManagedArtifactKind::RootDisk,
            "Manifest updated.",
        );

        let paths = ManagedImagePaths::from_records(spec, &resolved_paths)?;

        Ok(ManagedImageEnsureOutcome { paths, events })
    }

    fn push_event(
        &self,
        spec: &ManagedImageSpec,
        events: &mut Vec<ManagedArtifactEvent>,
        artifact: ManagedArtifactKind,
        message: impl Into<String>,
    ) {
        let event = ManagedArtifactEvent::new(artifact, message);
        self.log_event(spec, &event);
        events.push(event);
    }

    fn log_event(&self, spec: &ManagedImageSpec, event: &ManagedArtifactEvent) {
        let line = format!(
            "[{}] {} [{}] {}",
            timestamp_seconds(),
            spec.identifier(),
            event.artifact.describe(),
            event.message
        );
        self.log_line(&line);
    }

    fn log_line(&self, line: &str) {
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
        {
            let _ = writeln!(file, "{line}");
        }
    }

    fn download_artifact(
        &self,
        spec: &ManagedImageSpec,
        image_root: &Path,
        artifact: &ManagedArtifactSpec,
        cache_state: &CacheState,
        events: &mut Vec<ManagedArtifactEvent>,
    ) -> Result<PathBuf> {
        let partial = image_root.join(format!("{}.partial", artifact.final_filename));
        let mut start = 0u64;

        if partial.exists() {
            start = fs::metadata(&partial).map(|meta| meta.len()).unwrap_or(0);
        }

        let mut request = self.agent.get(artifact.source.url);
        if start > 0 {
            request = request.set("Range", &format!("bytes={start}-"));
        }

        self.push_event(
            spec,
            events,
            artifact.kind,
            format!(
                "{}: downloading from {} (resume offset {}).",
                artifact.kind.describe(),
                artifact.source.url,
                start
            ),
        );

        let response = request
            .call()
            .map_err(|err| Self::map_download_error(&artifact.source.url, err, cache_state))?;

        if start > 0 && response.status() == 200 {
            // Server ignored the Range header; start fresh.
            start = 0;
        }

        let mut file = if start > 0 {
            OpenOptions::new()
                .append(true)
                .open(&partial)
                .map_err(|err| Error::PreflightFailed {
                    message: format!(
                        "Failed to open partial download {}: {err}",
                        partial.display()
                    ),
                })?
        } else {
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&partial)
                .map_err(|err| Error::PreflightFailed {
                    message: format!(
                        "Failed to create download file {}: {err}",
                        partial.display()
                    ),
                })?
        };

        let mut reader = response.into_reader();
        let mut buffer = [0u8; 8192];
        loop {
            let bytes = reader
                .read(&mut buffer)
                .map_err(|err| Error::PreflightFailed {
                    message: format!("I/O error while downloading {}: {err}", artifact.source.url),
                })?;
            if bytes == 0 {
                break;
            }
            file.write_all(&buffer[..bytes])
                .map_err(|err| Error::PreflightFailed {
                    message: format!("Failed writing to download {}: {err}", partial.display()),
                })?;
        }

        let downloaded_size = fs::metadata(&partial)
            .map(|meta| meta.len())
            .unwrap_or_default();
        self.push_event(
            spec,
            events,
            artifact.kind,
            format!(
                "{}: download complete ({} bytes).",
                artifact.kind.describe(),
                downloaded_size
            ),
        );

        if let Some(expected) = artifact.source.size {
            let actual = fs::metadata(&partial)
                .map(|meta| meta.len())
                .unwrap_or_default();
            if actual != expected {
                let _ = fs::remove_file(&partial);
                return Err(Error::PreflightFailed {
                    message: format!(
                        "Downloaded {} but size {} did not match expected {} bytes. Removed {} so it can be fetched afresh.",
                        artifact.source.url,
                        actual,
                        expected,
                        partial.display()
                    ),
                });
            }
        }

        Ok(partial)
    }

    fn map_download_error(url: &str, err: UreqError, cache_state: &CacheState) -> Error {
        match err {
            UreqError::Status(status, response) => {
                let status_text = response.status_text().to_string();
                Error::PreflightFailed {
                    message: format!(
                        "Server responded with HTTP {status} {status_text} while downloading {url}.",
                    ),
                }
            }
            UreqError::Transport(transport) => {
                let kind = transport.kind();
                let detail = transport
                    .message()
                    .map(|msg| msg.to_string())
                    .unwrap_or_else(|| transport.to_string());
                let (base, hint) = match kind {
                    UreqErrorKind::Dns
                    | UreqErrorKind::ConnectionFailed
                    | UreqErrorKind::Io
                    | UreqErrorKind::ProxyConnect => (
                        format!("Network unavailable while downloading {url}"),
                        Self::offline_hint(cache_state),
                    ),
                    UreqErrorKind::InvalidProxyUrl | UreqErrorKind::ProxyUnauthorized => (
                        format!("Proxy configuration blocked the download of {url}"),
                        "Review Castra proxy settings or environment variables to proceed."
                            .to_string(),
                    ),
                    UreqErrorKind::InsecureRequestHttpsOnly => (
                        format!("Download rejected due to HTTPS-only policy for {url}"),
                        "Use an HTTPS endpoint or relax the https-only setting if intentional."
                            .to_string(),
                    ),
                    _ => (
                        format!("Failed to download {url}"),
                        "Retry once the remote endpoint is reachable.".to_string(),
                    ),
                };
                Error::PreflightFailed {
                    message: format!("{base}: {detail}. {hint}"),
                }
            }
        }
    }

    fn offline_hint(cache_state: &CacheState) -> String {
        match cache_state {
            CacheState::Missing => "No verified cache exists; operation cannot proceed offline—connect to the internet or prefetch the image before retrying `castra up`.".to_string(),
            CacheState::Stale(path) => format!(
                "Cached artifact at {} could not be verified; remove it or restore connectivity so Castra can refresh it (operation cannot proceed offline).",
                path.display()
            ),
        }
    }

    fn apply_transformations(
        &self,
        spec: &ManagedImageSpec,
        image_root: &Path,
        artifact: &ManagedArtifactSpec,
        mut current: PathBuf,
        events: &mut Vec<ManagedArtifactEvent>,
    ) -> Result<PathBuf> {
        for step in artifact.transformations {
            match step {
                TransformStep::StripVhdFooter => {
                    strip_vhd_footer(&current)?;
                    self.push_event(spec, events, artifact.kind, "VHD footer stripped.");
                }
                TransformStep::QemuImgConvert {
                    input_format,
                    output_format,
                    output,
                } => {
                    let qemu_img = self.qemu_img.as_ref().ok_or_else(|| Error::PreflightFailed {
                        message: "qemu-img binary required for managed image conversion but not found in PATH.".to_string(),
                    })?;
                    let target = image_root.join(output);
                    run_qemu_img_convert(qemu_img, input_format, output_format, &current, &target)?;
                    self.push_event(
                        spec,
                        events,
                        artifact.kind,
                        format!(
                            "Converted via qemu-img ({}→{}).",
                            input_format, output_format
                        ),
                    );
                    current = target;
                }
                TransformStep::Rename { output } => {
                    let target = image_root.join(output);
                    fs::rename(&current, &target).map_err(|err| Error::PreflightFailed {
                        message: format!(
                            "Failed to rename {} to {}: {err}",
                            current.display(),
                            target.display()
                        ),
                    })?;
                    self.push_event(
                        spec,
                        events,
                        artifact.kind,
                        format!("Renamed to {}.", target.display()),
                    );
                    current = target;
                }
            }
        }

        Ok(current)
    }
}

#[derive(Debug)]
pub struct ManagedImageSpec {
    pub id: &'static str,
    pub version: &'static str,
    pub artifacts: &'static [ManagedArtifactSpec],
    pub qemu: QemuProfile,
}

impl ManagedImageSpec {
    fn identifier(&self) -> String {
        format!("{}@{}", self.id, self.version)
    }

    fn fingerprint(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.id.as_bytes());
        hasher.update(self.version.as_bytes());
        for artifact in self.artifacts {
            hasher.update(artifact.kind.describe().as_bytes());
            hasher.update(artifact.final_filename.as_bytes());
            hasher.update(artifact.source.url.as_bytes());
            if let Some(sum) = artifact.source.sha256 {
                hasher.update(sum.as_bytes());
            }
            for transform in artifact.transformations {
                hasher.update(transform.fingerprint().as_bytes());
            }
        }
        hex_encode(hasher.finalize())
    }
}

#[derive(Debug)]
pub struct QemuProfile {
    pub kernel: Option<ManagedArtifactKind>,
    pub initrd: Option<ManagedArtifactKind>,
    pub append: &'static str,
    pub machine: Option<&'static str>,
    pub extra_args: &'static [&'static str],
}

#[derive(Debug)]
pub struct ManagedArtifactSpec {
    pub kind: ManagedArtifactKind,
    pub final_filename: &'static str,
    pub source: ArtifactSource,
    pub transformations: &'static [TransformStep],
}

#[derive(Debug)]
pub struct ArtifactSource {
    pub url: &'static str,
    pub sha256: Option<&'static str>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ManagedArtifactKind {
    RootDisk,
    Kernel,
    Initrd,
}

impl ManagedArtifactKind {
    pub fn describe(&self) -> &'static str {
        match self {
            ManagedArtifactKind::RootDisk => "root disk",
            ManagedArtifactKind::Kernel => "kernel",
            ManagedArtifactKind::Initrd => "initrd",
        }
    }
}

#[derive(Debug)]
pub enum TransformStep {
    #[allow(dead_code)]
    StripVhdFooter,
    QemuImgConvert {
        input_format: &'static str,
        output_format: &'static str,
        output: &'static str,
    },
    #[allow(dead_code)]
    Rename { output: &'static str },
}

impl TransformStep {
    fn fingerprint(&self) -> String {
        match self {
            TransformStep::StripVhdFooter => "strip_vhd".to_string(),
            TransformStep::QemuImgConvert {
                input_format,
                output_format,
                output,
            } => format!("convert:{input_format}:{output_format}:{output}"),
            TransformStep::Rename { output } => format!("rename:{output}"),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ImageManifest {
    spec_digest: Option<String>,
    last_checked: Option<u64>,
    artifacts: HashMap<String, ManifestArtifact>,
}

impl ImageManifest {
    fn new(spec_digest: String) -> Self {
        Self {
            spec_digest: Some(spec_digest),
            last_checked: None,
            artifacts: HashMap::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ManifestArtifact {
    final_sha256: String,
    size: u64,
    updated_at: u64,
    source_sha256: Option<String>,
}

fn load_manifest(path: &Path) -> ImageManifest {
    if let Ok(contents) = fs::read_to_string(path) {
        serde_json::from_str(&contents).unwrap_or_default()
    } else {
        ImageManifest::default()
    }
}

fn save_manifest(path: &Path, manifest: &ImageManifest) -> Result<()> {
    let serialized =
        serde_json::to_string_pretty(manifest).map_err(|err| Error::PreflightFailed {
            message: format!(
                "Failed to serialize image manifest {}: {err}",
                path.display()
            ),
        })?;
    fs::write(path, serialized).map_err(|err| Error::PreflightFailed {
        message: format!("Failed to persist image manifest {}: {err}", path.display()),
    })
}

fn timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_secs()
}

fn compute_sha256(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|err| Error::PreflightFailed {
        message: format!("Failed to open {} for hashing: {err}", path.display()),
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let bytes = file
            .read(&mut buffer)
            .map_err(|err| Error::PreflightFailed {
                message: format!("Failed to read {} for hashing: {err}", path.display()),
            })?;
        if bytes == 0 {
            break;
        }
        hasher.update(&buffer[..bytes]);
    }

    Ok(hex_encode(hasher.finalize()))
}

fn verify_checksum(path: &Path, expected: &str) -> std::result::Result<(), String> {
    let actual = compute_sha256(path).map_err(|err| err.to_string())?;
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(format!(
            "Hash mismatch for {}: expected {}, found {}",
            path.display(),
            expected,
            actual
        ))
    }
}

fn strip_vhd_footer(path: &Path) -> Result<()> {
    let metadata = fs::metadata(path).map_err(|err| Error::PreflightFailed {
        message: format!("Unable to stat {}: {err}", path.display()),
    })?;
    let len = metadata.len();
    if len < 512 {
        return Err(Error::PreflightFailed {
            message: format!(
                "File {} too small to contain VHD footer ({} bytes).",
                path.display(),
                len
            ),
        });
    }
    let new_len = len - 512;
    let file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|err| Error::PreflightFailed {
            message: format!("Unable to open {} for truncation: {err}", path.display()),
        })?;
    file.set_len(new_len).map_err(|err| Error::PreflightFailed {
        message: format!(
            "Failed truncating {} to strip VHD footer: {err}",
            path.display()
        ),
    })
}

fn run_qemu_img_convert(
    qemu_img: &Path,
    input_format: &str,
    output_format: &str,
    input: &Path,
    output: &Path,
) -> Result<()> {
    if output.exists() {
        fs::remove_file(output).map_err(|err| Error::PreflightFailed {
            message: format!(
                "Failed to clear previous output {}: {err}",
                output.display()
            ),
        })?;
    }

    let status = Command::new(qemu_img)
        .arg("convert")
        .arg("-f")
        .arg(input_format)
        .arg("-O")
        .arg(output_format)
        .arg(input)
        .arg(output)
        .status()
        .map_err(|err| Error::PreflightFailed {
            message: format!("Failed to invoke `{}`: {err}", qemu_img.display()),
        })?;

    if !status.success() {
        return Err(Error::PreflightFailed {
            message: format!(
                "`{}` exited with code {} while converting {}.",
                qemu_img.display(),
                status.code().unwrap_or(-1),
                input.display()
            ),
        });
    }

    Ok(())
}

pub fn lookup_managed_image(id: &str, version: &str) -> Option<&'static ManagedImageSpec> {
    match (id, version) {
        ("alpine-minimal", "v1") => Some(&ALPINE_MINIMAL_V1),
        _ => None,
    }
}

static ALPINE_ARTIFACTS: [ManagedArtifactSpec; 3] = [
    ManagedArtifactSpec {
        kind: ManagedArtifactKind::RootDisk,
        final_filename: "rootfs.qcow2",
        source: ArtifactSource {
            url: "https://dl-cdn.alpinelinux.org/alpine/v3.22/releases/cloud/aws_alpine-3.22.2-x86_64-bios-tiny-r0.vhd",
            sha256: Some("8f58945cd972f31b8a7e3116d2b33cdb4298e6b3c0609c0bfd083964678afffb"),
            size: Some(127_926_784),
        },
        transformations: &[TransformStep::QemuImgConvert {
            input_format: "vpc",
            output_format: "qcow2",
            output: "rootfs.qcow2",
        }],
    },
    ManagedArtifactSpec {
        kind: ManagedArtifactKind::Kernel,
        final_filename: "vmlinuz-lts",
        source: ArtifactSource {
            url: "https://dl-cdn.alpinelinux.org/alpine/v3.22/releases/x86_64/netboot/vmlinuz-lts",
            sha256: Some("6eb498e1898d138e8a493eae901a580ddc3c1c105bd9ddc84cb9f820855958e7"),
            size: Some(13_624_320),
        },
        transformations: &[],
    },
    ManagedArtifactSpec {
        kind: ManagedArtifactKind::Initrd,
        final_filename: "initramfs-lts",
        source: ArtifactSource {
            url: "https://dl-cdn.alpinelinux.org/alpine/v3.22/releases/x86_64/netboot/initramfs-lts",
            sha256: Some("e82c5c2d4a6372f25e53fcbad4defe7b10bbf8be766b6e571291fd5ebcf9e383"),
            size: Some(26_335_797),
        },
        transformations: &[],
    },
];

static ALPINE_MINIMAL_V1: ManagedImageSpec = ManagedImageSpec {
    id: "alpine-minimal",
    version: "v1",
    artifacts: &ALPINE_ARTIFACTS,
    qemu: QemuProfile {
        kernel: Some(ManagedArtifactKind::Kernel),
        initrd: Some(ManagedArtifactKind::Initrd),
        append: "console=ttyS0 root=/dev/vda modules=virtio_pci,virtio_blk,virtio_console,virtio_net,ext4 quiet",
        machine: None,
        extra_args: &[],
    },
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::{NamedTempFile, tempdir};

    #[test]
    fn managed_artifact_kind_describe_matches_variants() {
        assert_eq!(ManagedArtifactKind::RootDisk.describe(), "root disk");
        assert_eq!(ManagedArtifactKind::Kernel.describe(), "kernel");
        assert_eq!(ManagedArtifactKind::Initrd.describe(), "initrd");
    }

    #[test]
    fn transform_step_fingerprint_varies_by_variant() {
        assert_eq!(TransformStep::StripVhdFooter.fingerprint(), "strip_vhd");
        assert_eq!(
            TransformStep::QemuImgConvert {
                input_format: "raw",
                output_format: "qcow2",
                output: "disk.qcow2"
            }
            .fingerprint(),
            "convert:raw:qcow2:disk.qcow2"
        );
        assert_eq!(
            TransformStep::Rename {
                output: "disk.qcow2"
            }
            .fingerprint(),
            "rename:disk.qcow2"
        );
    }

    const TEST_PROFILE: QemuProfile = QemuProfile {
        kernel: None,
        initrd: None,
        append: "",
        machine: None,
        extra_args: &[],
    };

    #[test]
    fn managed_image_spec_fingerprint_changes_with_artifacts() {
        static ARTIFACTS_A: [ManagedArtifactSpec; 1] = [ManagedArtifactSpec {
            kind: ManagedArtifactKind::RootDisk,
            final_filename: "a.img",
            source: ArtifactSource {
                url: "https://example.com/a.img",
                sha256: Some("abc"),
                size: None,
            },
            transformations: &[],
        }];
        static ARTIFACTS_B: [ManagedArtifactSpec; 1] = [ManagedArtifactSpec {
            kind: ManagedArtifactKind::RootDisk,
            final_filename: "b.img",
            source: ArtifactSource {
                url: "https://example.com/a.img",
                sha256: Some("abc"),
                size: None,
            },
            transformations: &[],
        }];
        let spec_a = ManagedImageSpec {
            id: "demo",
            version: "1",
            artifacts: &ARTIFACTS_A,
            qemu: TEST_PROFILE,
        };
        let spec_b = ManagedImageSpec {
            id: "demo",
            version: "1",
            artifacts: &ARTIFACTS_B,
            qemu: TEST_PROFILE,
        };
        let fingerprint_a = spec_a.fingerprint();
        let fingerprint_b = spec_b.fingerprint();
        assert_ne!(fingerprint_a, fingerprint_b);
    }

    #[test]
    fn lookup_managed_image_finds_known_spec() {
        let spec = lookup_managed_image("alpine-minimal", "v1").expect("known spec");
        assert_eq!(spec.id, "alpine-minimal");
        assert_eq!(spec.version, "v1");
        assert!(matches!(
            spec.qemu.kernel,
            Some(ManagedArtifactKind::Kernel)
        ));
        assert!(matches!(
            spec.qemu.initrd,
            Some(ManagedArtifactKind::Initrd)
        ));
        assert!(!spec.qemu.append.is_empty());
        assert_eq!(spec.artifacts.len(), 3);
    }

    #[test]
    fn save_and_load_manifest_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        let mut manifest = ImageManifest::default();
        manifest.spec_digest = Some("digest".into());
        manifest.artifacts.insert(
            "disk".into(),
            ManifestArtifact {
                final_sha256: "abc".into(),
                size: 42,
                updated_at: 123,
                source_sha256: None,
            },
        );
        save_manifest(&path, &manifest).unwrap();
        let loaded = load_manifest(&path);
        assert_eq!(loaded.spec_digest, Some("digest".into()));
        assert!(loaded.artifacts.contains_key("disk"));
    }

    #[test]
    fn timestamp_seconds_returns_positive_value() {
        assert!(timestamp_seconds() > 0);
    }

    #[test]
    fn compute_and_verify_sha256_succeeds() {
        let file = NamedTempFile::new().unwrap();
        write!(file.as_file(), "hello").unwrap();
        let hash = compute_sha256(file.path()).unwrap();
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        verify_checksum(
            file.path(),
            "2CF24DBA5FB0A30E26E83B2AC5B9E29E1B161E5C1FA7425E73043362938B9824",
        )
        .expect("checksum should verify ignoring case");
    }

    #[test]
    fn strip_vhd_footer_truncates_file() {
        let file = NamedTempFile::new().unwrap();
        file.as_file().write_all(&vec![0u8; 1024]).expect("write");
        strip_vhd_footer(file.path()).expect("strip footer");
        let metadata = fs::metadata(file.path()).unwrap();
        assert_eq!(metadata.len(), 512);
    }

    #[cfg(unix)]
    #[test]
    fn run_qemu_img_convert_invokes_stub() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let script_path = dir.path().join("qemu-img");
        let args_path = dir.path().join("args.txt");
        fs::write(
            &script_path,
            format!(
                "#!/bin/sh\nprintf \"%s \" \"$@\" > {}\ncp \"$6\" \"$7\"\n",
                args_path.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();

        let input = dir.path().join("input.raw");
        let output = dir.path().join("output.qcow2");
        fs::write(&input, b"data").unwrap();

        run_qemu_img_convert(&script_path, "raw", "qcow2", &input, &output).unwrap();
        let logged = fs::read_to_string(&args_path).unwrap();
        assert!(logged.contains("convert"));
        assert!(logged.contains("-f raw"));
        assert!(logged.contains("-O qcow2"));
        assert!(output.is_file());
    }

    #[test]
    fn run_qemu_img_convert_errors_when_missing_binary() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("input.raw");
        let output = dir.path().join("output.qcow2");
        fs::write(&input, b"data").unwrap();
        let missing = dir.path().join("missing");
        let err = run_qemu_img_convert(&missing, "raw", "qcow2", &input, &output).unwrap_err();
        assert!(err.to_string().contains("Failed to invoke"));
    }
}
