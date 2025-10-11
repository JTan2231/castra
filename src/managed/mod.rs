use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use hex::encode as hex_encode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ureq::Agent;

use crate::error::{CliError, CliResult};

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

#[derive(Debug)]
pub struct ManagedImagePaths {
    pub root_disk: PathBuf,
    pub kernel: Option<PathBuf>,
    pub initrd: Option<PathBuf>,
}

impl ManagedImagePaths {
    fn from_records(
        spec: &'static ManagedImageSpec,
        records: &HashMap<ManagedArtifactKind, PathBuf>,
    ) -> CliResult<Self> {
        let root_disk = records
            .get(&ManagedArtifactKind::RootDisk)
            .cloned()
            .ok_or_else(|| CliError::PreflightFailed {
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
    ) -> CliResult<ManagedImageEnsureOutcome> {
        let image_root = self.storage_root.join(spec.id).join(spec.version);
        fs::create_dir_all(&image_root).map_err(|err| CliError::PreflightFailed {
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

            self.push_event(
                spec,
                &mut events,
                artifact.kind,
                format!("{}: refreshing cached artifact.", artifact.kind.describe()),
            );

            let download_path = self.download_artifact(spec, &image_root, artifact, &mut events)?;
            if let Some(expected) = artifact.source.sha256 {
                verify_checksum(&download_path, expected).map_err(|err| {
                    CliError::PreflightFailed {
                        message: format!(
                            "Checksum mismatch for {} (expected {expected}): {err}",
                            artifact.source.url
                        ),
                    }
                })?;
                self.push_event(
                    spec,
                    &mut events,
                    artifact.kind,
                    "source checksum verified.",
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
                    CliError::PreflightFailed {
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
        events: &mut Vec<ManagedArtifactEvent>,
    ) -> CliResult<PathBuf> {
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

        let response = request.call().map_err(|err| CliError::PreflightFailed {
            message: format!("Failed to download {}: {err}", artifact.source.url),
        })?;

        if start > 0 && response.status() == 200 {
            // Server ignored the Range header; start fresh.
            start = 0;
        }

        let mut file = if start > 0 {
            OpenOptions::new()
                .append(true)
                .open(&partial)
                .map_err(|err| CliError::PreflightFailed {
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
                .map_err(|err| CliError::PreflightFailed {
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
                .map_err(|err| CliError::PreflightFailed {
                    message: format!("I/O error while downloading {}: {err}", artifact.source.url),
                })?;
            if bytes == 0 {
                break;
            }
            file.write_all(&buffer[..bytes])
                .map_err(|err| CliError::PreflightFailed {
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
                return Err(CliError::PreflightFailed {
                    message: format!(
                        "Downloaded {} but size {} did not match expected {} bytes.",
                        artifact.source.url, actual, expected
                    ),
                });
            }
        }

        Ok(partial)
    }

    fn apply_transformations(
        &self,
        spec: &ManagedImageSpec,
        image_root: &Path,
        artifact: &ManagedArtifactSpec,
        mut current: PathBuf,
        events: &mut Vec<ManagedArtifactEvent>,
    ) -> CliResult<PathBuf> {
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
                    let qemu_img = self.qemu_img.as_ref().ok_or_else(|| CliError::PreflightFailed {
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
                    fs::rename(&current, &target).map_err(|err| CliError::PreflightFailed {
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
    #[allow(dead_code)]
    Kernel,
    #[allow(dead_code)]
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
    Rename {
        output: &'static str,
    },
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

fn save_manifest(path: &Path, manifest: &ImageManifest) -> CliResult<()> {
    let serialized =
        serde_json::to_string_pretty(manifest).map_err(|err| CliError::PreflightFailed {
            message: format!(
                "Failed to serialize image manifest {}: {err}",
                path.display()
            ),
        })?;
    fs::write(path, serialized).map_err(|err| CliError::PreflightFailed {
        message: format!("Failed to persist image manifest {}: {err}", path.display()),
    })
}

fn timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_secs()
}

fn compute_sha256(path: &Path) -> CliResult<String> {
    let mut file = fs::File::open(path).map_err(|err| CliError::PreflightFailed {
        message: format!("Failed to open {} for hashing: {err}", path.display()),
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let bytes = file
            .read(&mut buffer)
            .map_err(|err| CliError::PreflightFailed {
                message: format!("Failed to read {} for hashing: {err}", path.display()),
            })?;
        if bytes == 0 {
            break;
        }
        hasher.update(&buffer[..bytes]);
    }

    Ok(hex_encode(hasher.finalize()))
}

fn verify_checksum(path: &Path, expected: &str) -> Result<(), String> {
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

fn strip_vhd_footer(path: &Path) -> CliResult<()> {
    let metadata = fs::metadata(path).map_err(|err| CliError::PreflightFailed {
        message: format!("Unable to stat {}: {err}", path.display()),
    })?;
    let len = metadata.len();
    if len < 512 {
        return Err(CliError::PreflightFailed {
            message: format!(
                "File {} too small to contain VHD footer ({} bytes).",
                path.display(),
                len
            ),
        });
    }
    let new_len = len - 512;
    let file =
        OpenOptions::new()
            .write(true)
            .open(path)
            .map_err(|err| CliError::PreflightFailed {
                message: format!("Unable to open {} for truncation: {err}", path.display()),
            })?;
    file.set_len(new_len)
        .map_err(|err| CliError::PreflightFailed {
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
) -> CliResult<()> {
    if output.exists() {
        fs::remove_file(output).map_err(|err| CliError::PreflightFailed {
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
        .map_err(|err| CliError::PreflightFailed {
            message: format!("Failed to invoke `{}`: {err}", qemu_img.display()),
        })?;

    if !status.success() {
        return Err(CliError::PreflightFailed {
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

static ALPINE_ARTIFACTS: [ManagedArtifactSpec; 1] = [
    ManagedArtifactSpec {
        kind: ManagedArtifactKind::RootDisk,
        final_filename: "rootfs.qcow2",
        source: ArtifactSource {
            url: "https://dl-cdn.alpinelinux.org/alpine/v3.22/releases/cloud/aws_alpine-3.22.2-x86_64-bios-tiny-r0.vhd",
            sha256: None,
            size: None,
        },
        transformations: &[
            TransformStep::QemuImgConvert {
                input_format: "vpc",
                output_format: "qcow2",
                output: "rootfs.qcow2",
            },
        ],
    },
];

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
