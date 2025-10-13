use std::path::PathBuf;

/// Source used when resolving a Castra configuration.
#[derive(Debug, Clone)]
pub enum ConfigSource {
    /// Search for `castra.toml` by walking up from the current working directory.
    Discover,
    /// Use an explicit path to the configuration file.
    Explicit(PathBuf),
}

/// Parameters for configuration loading and optional synthetic project creation.
#[derive(Debug, Clone)]
pub struct ConfigLoadOptions {
    /// Where to source the configuration from.
    pub source: ConfigSource,
    /// Whether the loader may return a synthetic default project when nothing is found.
    pub allow_synthetic: bool,
    /// Optional override for the discovery root (defaults to the process CWD).
    pub search_root: Option<PathBuf>,
}

impl ConfigLoadOptions {
    /// Convenience constructor for explicit config usage.
    pub fn explicit(path: PathBuf) -> Self {
        Self {
            source: ConfigSource::Explicit(path),
            allow_synthetic: false,
            search_root: None,
        }
    }

    /// Convenience constructor for discovery with optional synthesis.
    pub fn discover(allow_synthetic: bool) -> Self {
        Self {
            source: ConfigSource::Discover,
            allow_synthetic,
            search_root: None,
        }
    }
}

/// Options accepted by the `init` operation.
#[derive(Debug, Clone)]
pub struct InitOptions {
    /// Whether an existing file should be overwritten.
    pub force: bool,
    /// Optional project name for the generated configuration.
    pub project_name: Option<String>,
    /// Preferred output path for the configuration. When absent the value is derived from `config_hint`.
    pub output_path: Option<PathBuf>,
    /// Hint from the caller (e.g. `--config`) that influences the default output path.
    pub config_hint: ConfigSource,
}

impl Default for InitOptions {
    fn default() -> Self {
        Self {
            force: false,
            project_name: None,
            output_path: None,
            config_hint: ConfigSource::Discover,
        }
    }
}

/// Options for the `up` operation.
#[derive(Debug, Clone)]
pub struct UpOptions {
    /// Configuration lookup parameters.
    pub config: ConfigLoadOptions,
    /// Whether to force operations even if host checks fail.
    pub force: bool,
}

impl Default for UpOptions {
    fn default() -> Self {
        Self {
            config: ConfigLoadOptions::discover(true),
            force: false,
        }
    }
}

/// Options for the `down` operation.
#[derive(Debug, Clone)]
pub struct DownOptions {
    /// Configuration lookup parameters.
    pub config: ConfigLoadOptions,
}

impl Default for DownOptions {
    fn default() -> Self {
        Self {
            config: ConfigLoadOptions::discover(true),
        }
    }
}

/// Options for the `status` operation.
#[derive(Debug, Clone)]
pub struct StatusOptions {
    /// Configuration lookup parameters.
    pub config: ConfigLoadOptions,
}

impl Default for StatusOptions {
    fn default() -> Self {
        Self {
            config: ConfigLoadOptions::discover(true),
        }
    }
}

/// Options for the `ports` operation.
#[derive(Debug, Clone)]
pub struct PortsOptions {
    /// Configuration lookup parameters.
    pub config: ConfigLoadOptions,
    /// Whether to include inactive forwards.
    pub verbose: bool,
    /// Which ports view to render.
    pub view: PortsView,
}

impl Default for PortsOptions {
    fn default() -> Self {
        Self {
            config: ConfigLoadOptions::discover(true),
            verbose: false,
            view: PortsView::Declared,
        }
    }
}

/// View mode requested for the ports surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortsView {
    /// Show declared forwards without inspecting runtime state.
    Declared,
    /// Inspect runtime state and mark forwards as active when their VM is running.
    Active,
}

/// Options for the `logs` operation.
#[derive(Debug, Clone)]
pub struct LogsOptions {
    /// Configuration lookup parameters.
    pub config: ConfigLoadOptions,
    /// Number of historical lines to show before following.
    pub tail: usize,
    /// Whether to follow logs continuously.
    pub follow: bool,
}

impl Default for LogsOptions {
    fn default() -> Self {
        Self {
            config: ConfigLoadOptions::discover(true),
            tail: 200,
            follow: false,
        }
    }
}

/// Options for the `clean` operation.
#[derive(Debug, Clone)]
pub struct CleanOptions {
    /// Scope describing which state roots should be cleaned.
    pub scope: CleanScope,
    /// Preview cleanup actions without deleting files.
    pub dry_run: bool,
    /// Include VM overlays declared in the project.
    pub include_overlays: bool,
    /// Include orchestrator logs directory.
    pub include_logs: bool,
    /// Include broker handshake artifacts.
    pub include_handshakes: bool,
    /// Restrict cleanup to managed image artifacts.
    pub managed_only: bool,
    /// Override running-process safeguards.
    pub force: bool,
}

/// Scope selector for the clean command.
#[derive(Debug, Clone)]
pub enum CleanScope {
    /// Operate on all state roots under the shared projects directory.
    Global { projects_root: PathBuf },
    /// Operate on a single workspace, resolved via config or explicit state root.
    Workspace(ProjectSelector),
}

/// Workspace selection strategy.
#[derive(Debug, Clone)]
pub enum ProjectSelector {
    /// Resolve the workspace via config lookup.
    Config(ConfigLoadOptions),
    /// Use the provided state root directly.
    StateRoot(PathBuf),
}

/// Options for the hidden `broker` command exposed via the library API.
#[derive(Debug, Clone)]
pub struct BrokerOptions {
    /// Port to bind the broker to.
    pub port: u16,
    /// Broker PID file path.
    pub pidfile: PathBuf,
    /// Log file path for the broker.
    pub logfile: PathBuf,
    /// Directory where broker ↔ guest handshake artifacts are recorded.
    pub handshake_dir: PathBuf,
}
