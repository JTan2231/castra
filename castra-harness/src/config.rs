use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::vizier_remote::VizierRemoteConfig;

/// Configuration for spawning and supervising Codex.
#[derive(Clone, Debug)]
pub struct HarnessConfig {
    binary_path: PathBuf,
    model: Option<String>,
    default_resume_thread: Option<String>,
    working_dir: Option<PathBuf>,
    env: BTreeMap<String, String>,
    persist_history: bool,
    history_root: Option<PathBuf>,
    vizier_remote: VizierRemoteConfig,
}

impl HarnessConfig {
    pub fn new<P: Into<PathBuf>>(binary_path: P) -> Self {
        Self {
            binary_path: binary_path.into(),
            model: None,
            default_resume_thread: None,
            working_dir: None,
            env: BTreeMap::new(),
            persist_history: false,
            history_root: None,
            vizier_remote: VizierRemoteConfig::default(),
        }
    }

    pub fn binary_path(&self) -> &Path {
        &self.binary_path
    }

    pub fn set_binary_path<P: Into<PathBuf>>(&mut self, path: P) {
        self.binary_path = path.into();
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn set_model<S: Into<String>>(&mut self, model: S) {
        self.model = Some(model.into());
    }

    pub fn clear_model(&mut self) {
        self.model = None;
    }

    pub fn default_resume_thread(&self) -> Option<&str> {
        self.default_resume_thread.as_deref()
    }

    pub fn set_default_resume_thread<S: Into<String>>(&mut self, id: S) {
        self.default_resume_thread = Some(id.into());
    }

    pub fn clear_default_resume_thread(&mut self) {
        self.default_resume_thread = None;
    }

    pub fn working_dir(&self) -> Option<&Path> {
        self.working_dir.as_deref()
    }

    pub fn set_working_dir<P: Into<PathBuf>>(&mut self, path: P) {
        self.working_dir = Some(path.into());
    }

    pub fn clear_working_dir(&mut self) {
        self.working_dir = None;
    }

    pub fn env(&self) -> &BTreeMap<String, String> {
        &self.env
    }

    pub fn env_mut(&mut self) -> &mut BTreeMap<String, String> {
        &mut self.env
    }

    pub fn set_env_var<K, V>(&mut self, key: K, value: V)
    where
        K: Into<String>,
        V: Into<String>,
    {
        self.env.insert(key.into(), value.into());
    }

    pub fn remove_env_var(&mut self, key: &str) {
        self.env.remove(key);
    }

    pub fn persist_history(&self) -> bool {
        self.persist_history
    }

    pub fn enable_history<P: Into<PathBuf>>(&mut self, root: P) {
        self.persist_history = true;
        self.history_root = Some(root.into());
    }

    pub fn disable_history(&mut self) {
        self.persist_history = false;
        self.history_root = None;
    }

    pub fn history_root(&self) -> Option<&Path> {
        self.history_root.as_deref()
    }

    pub fn merge_env(&mut self, other: &BTreeMap<String, String>) {
        for (key, value) in other {
            self.env.insert(key.clone(), value.clone());
        }
    }

    pub fn vizier_remote(&self) -> &VizierRemoteConfig {
        &self.vizier_remote
    }

    pub fn vizier_remote_mut(&mut self) -> &mut VizierRemoteConfig {
        &mut self.vizier_remote
    }
}

/// Parameters for a single Codex turn.
#[derive(Clone, Debug)]
pub struct TurnRequest {
    prompt: String,
    resume_thread: Option<String>,
    model: Option<String>,
}

impl TurnRequest {
    pub fn new<S: Into<String>>(prompt: S) -> Self {
        Self {
            prompt: prompt.into(),
            resume_thread: None,
            model: None,
        }
    }

    pub fn with_resume_thread<S: Into<String>>(mut self, thread_id: S) -> Self {
        self.resume_thread = Some(thread_id.into());
        self
    }

    pub fn with_model<S: Into<String>>(mut self, model: S) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    pub fn resume_thread(&self) -> Option<&str> {
        self.resume_thread.as_deref()
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }
}
