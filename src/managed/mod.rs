use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use hex::encode as hex_encode;
use serde::{Deserialize, Serialize};
use serde_json::json;
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
    pub verification: ManagedImageVerification,
}

#[derive(Debug, Clone)]
pub struct ManagedImagePaths {
    pub root_disk: PathBuf,
    pub kernel: Option<PathBuf>,
    pub initrd: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ManagedImageArtifactSummary {
    pub kind: ManagedArtifactKind,
    pub filename: String,
    pub size_bytes: u64,
    pub final_sha256: String,
    pub source_sha256: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ManagedImageVerification {
    pub artifacts: Vec<ManagedImageArtifactSummary>,
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

#[derive(Debug, Clone)]
pub struct ManagedArtifactEvent {
    pub artifact: ManagedArtifactKind,
    pub detail: ManagedArtifactEventDetail,
    pub message: String,
}

impl ManagedArtifactEvent {
    fn new(artifact: ManagedArtifactKind, detail: ManagedArtifactEventDetail) -> Self {
        let message = detail.render(artifact);
        Self {
            artifact,
            detail,
            message,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagedArtifactEventDetail {
    CacheHit,
    RefreshingCache,
    DownloadStarted {
        url: String,
        resume_offset: u64,
    },
    DownloadCompleted {
        bytes: u64,
    },
    SourceChecksumVerified,
    FinalChecksumRecorded {
        checksum: String,
    },
    ManifestUpdated,
    TransformVhdFooterStripped,
    TransformQemuImgConvert {
        input_format: String,
        output_format: String,
    },
    TransformRename {
        target: String,
    },
}

impl ManagedArtifactEventDetail {
    fn render(&self, artifact: ManagedArtifactKind) -> String {
        match self {
            Self::CacheHit => format!("{}: cache hit (verified).", artifact.describe()),
            Self::RefreshingCache => {
                format!("{}: refreshing cached artifact.", artifact.describe())
            }
            Self::DownloadStarted { url, resume_offset } => format!(
                "{}: downloading from {} (resume offset {}).",
                artifact.describe(),
                url,
                resume_offset
            ),
            Self::DownloadCompleted { bytes } => format!(
                "{}: download complete ({} bytes).",
                artifact.describe(),
                bytes
            ),
            Self::SourceChecksumVerified => "verified source checksums.".to_string(),
            Self::FinalChecksumRecorded { checksum } => format!(
                "{}: final checksum {} stored.",
                artifact.describe(),
                checksum
            ),
            Self::ManifestUpdated => "Manifest updated.".to_string(),
            Self::TransformVhdFooterStripped => "VHD footer stripped.".to_string(),
            Self::TransformQemuImgConvert {
                input_format,
                output_format,
            } => format!(
                "Converted via qemu-img ({}→{}).",
                input_format, output_format
            ),
            Self::TransformRename { target } => format!("Renamed to {target}."),
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
                            ManagedArtifactEventDetail::CacheHit,
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
                ManagedArtifactEventDetail::RefreshingCache,
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
                    ManagedArtifactEventDetail::SourceChecksumVerified,
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
                ManagedArtifactEventDetail::FinalChecksumRecorded {
                    checksum: final_hash.clone(),
                },
            );

            resolved_paths.insert(artifact.kind, final_location);
        }

        manifest.last_checked = Some(timestamp_seconds());
        let verification = ManagedImageVerification {
            artifacts: spec
                .artifacts
                .iter()
                .filter_map(|artifact| {
                    manifest
                        .artifacts
                        .get(artifact.final_filename)
                        .map(|record| ManagedImageArtifactSummary {
                            kind: artifact.kind,
                            filename: artifact.final_filename.to_string(),
                            size_bytes: record.size,
                            final_sha256: record.final_sha256.clone(),
                            source_sha256: record.source_sha256.clone(),
                        })
                })
                .collect(),
        };
        save_manifest(&manifest_path, &manifest)?;
        self.log_verification(spec, &verification);

        self.push_event(
            spec,
            &mut events,
            ManagedArtifactKind::RootDisk,
            ManagedArtifactEventDetail::ManifestUpdated,
        );

        let paths = ManagedImagePaths::from_records(spec, &resolved_paths)?;

        Ok(ManagedImageEnsureOutcome {
            paths,
            events,
            verification,
        })
    }

    fn push_event(
        &self,
        spec: &ManagedImageSpec,
        events: &mut Vec<ManagedArtifactEvent>,
        artifact: ManagedArtifactKind,
        detail: ManagedArtifactEventDetail,
    ) {
        let event = ManagedArtifactEvent::new(artifact, detail);
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

    pub(crate) fn log_verification(
        &self,
        spec: &ManagedImageSpec,
        verification: &ManagedImageVerification,
    ) {
        let artifacts: Vec<_> = verification
            .artifacts
            .iter()
            .map(|artifact| {
                json!({
                    "kind": artifact.kind.describe(),
                    "filename": artifact.filename,
                    "size_bytes": artifact.size_bytes,
                    "final_sha256": artifact.final_sha256,
                    "source_sha256": artifact.source_sha256,
                })
            })
            .collect();
        let payload = json!({
            "ts": timestamp_seconds(),
            "event": "managed-image-verified",
            "image": spec.id,
            "version": spec.version,
            "artifacts": artifacts,
        });
        self.log_line(&payload.to_string());
    }

    pub fn log_profile_application(
        &self,
        spec: &ManagedImageSpec,
        vm: &str,
        kernel: &Path,
        initrd: Option<&Path>,
        append: &str,
        extra_args: &[String],
        machine: Option<&str>,
    ) {
        let payload = json!({
            "ts": timestamp_seconds(),
            "event": "managed-image-profile-applied",
            "image": spec.id,
            "version": spec.version,
            "vm": vm,
            "components": {
                "kernel": kernel.display().to_string(),
                "initrd": initrd.map(|path| path.display().to_string()),
                "append": append,
                "extra_args": extra_args.iter().cloned().collect::<Vec<String>>(),
                "machine": machine,
            },
        });
        self.log_line(&payload.to_string());
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
            ManagedArtifactEventDetail::DownloadStarted {
                url: artifact.source.url.to_string(),
                resume_offset: start,
            },
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
            ManagedArtifactEventDetail::DownloadCompleted {
                bytes: downloaded_size,
            },
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
                    self.push_event(
                        spec,
                        events,
                        artifact.kind,
                        ManagedArtifactEventDetail::TransformVhdFooterStripped,
                    );
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
                        ManagedArtifactEventDetail::TransformQemuImgConvert {
                            input_format: input_format.to_string(),
                            output_format: output_format.to_string(),
                        },
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
                        ManagedArtifactEventDetail::TransformRename {
                            target: target.display().to_string(),
                        },
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

static ALPINE_ARTIFACTS: [ManagedArtifactSpec; 1] = [ManagedArtifactSpec {
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
}];

static ALPINE_MINIMAL_V1: ManagedImageSpec = ManagedImageSpec {
    id: "alpine-minimal",
    version: "v1",
    artifacts: &ALPINE_ARTIFACTS,
    qemu: QemuProfile {
        kernel: None,
        initrd: None,
        append: "",
        machine: None,
        extra_args: &[],
    },
};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
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
        assert!(spec.qemu.kernel.is_none());
        assert!(spec.qemu.initrd.is_none());
        assert!(spec.qemu.append.is_empty());
        assert_eq!(spec.artifacts.len(), 1);
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

    #[test]
    fn log_verification_writes_json_line() {
        let dir = tempdir().unwrap();
        let storage_root = dir.path().join("storage");
        let log_root = dir.path().join("logs");
        fs::create_dir_all(&storage_root).unwrap();
        fs::create_dir_all(&log_root).unwrap();
        let manager = ImageManager::new(storage_root, log_root.clone(), None);

        let verification = ManagedImageVerification {
            artifacts: vec![ManagedImageArtifactSummary {
                kind: ManagedArtifactKind::RootDisk,
                filename: "disk.qcow2".to_string(),
                size_bytes: 10,
                final_sha256: "abc123".to_string(),
                source_sha256: Some("def456".to_string()),
            }],
        };

        manager.log_verification(&ALPINE_MINIMAL_V1, &verification);

        let log_path = log_root.join("image-manager.log");
        let contents = fs::read_to_string(&log_path).expect("log file");
        let line = contents.trim();
        let value: Value = serde_json::from_str(line).expect("json line");
        assert_eq!(value["event"], "managed-image-verified");
        assert_eq!(value["image"], ALPINE_MINIMAL_V1.id);
        assert_eq!(value["version"], ALPINE_MINIMAL_V1.version);
        assert_eq!(
            value["artifacts"][0]["filename"],
            verification.artifacts[0].filename
        );
        assert_eq!(
            value["artifacts"][0]["final_sha256"],
            verification.artifacts[0].final_sha256
        );
    }

    #[test]
    fn log_profile_application_writes_json_line() {
        let dir = tempdir().unwrap();
        let storage_root = dir.path().join("storage");
        let log_root = dir.path().join("logs");
        fs::create_dir_all(&storage_root).unwrap();
        fs::create_dir_all(&log_root).unwrap();
        let manager = ImageManager::new(storage_root, log_root.clone(), None);

        let kernel = dir.path().join("vmlinuz");
        let initrd = dir.path().join("initrd.img");
        let extra_args = vec!["arg1".to_string(), "arg2".to_string()];

        manager.log_profile_application(
            &ALPINE_MINIMAL_V1,
            "vm-test",
            &kernel,
            Some(&initrd),
            "console=ttyS0",
            &extra_args,
            Some("pc-q35"),
        );

        let log_path = log_root.join("image-manager.log");
        let contents = fs::read_to_string(&log_path).expect("log file");
        let line = contents.trim();
        let value: Value = serde_json::from_str(line).expect("json line");
        assert_eq!(value["event"], "managed-image-profile-applied");
        assert_eq!(value["vm"], "vm-test");
        assert_eq!(value["components"]["kernel"], kernel.display().to_string());
        assert_eq!(value["components"]["initrd"], initrd.display().to_string());
        assert_eq!(value["components"]["append"], "console=ttyS0");
        assert_eq!(value["components"]["extra_args"][0], "arg1");
        assert_eq!(value["components"]["machine"], "pc-q35");
    }
}
