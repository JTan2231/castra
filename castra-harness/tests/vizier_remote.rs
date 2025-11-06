use std::path::PathBuf;

use castra_harness::vizier_remote::{
    TunnelManager, VizierRemoteConfig, VizierRemoteEvent, VizierRemotePlan,
};

#[test]
fn vizier_remote_emits_handshake() {
    let binary = match locate_vizier_binary() {
        Some(path) => path,
        None => {
            eprintln!("Skipping vizier_remote_emits_handshake: castra-vizier binary not found");
            return;
        }
    };
    let plan = VizierRemotePlan::new(
        String::from("vm-test"),
        binary.to_string_lossy().into_owned(),
        vec!["--probe".to_string()],
    );

    let config = VizierRemoteConfig::default();
    let manager = TunnelManager::new(plan, config);
    let events = manager.events();

    let handshake = events.recv_blocking().expect("expected handshake event");

    match handshake {
        VizierRemoteEvent::Handshake { .. } => {}
        other => panic!("unexpected event: {:?}", other),
    }
}

fn locate_vizier_binary() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("CASTRA_VIZIER_BINARY") {
        return Some(PathBuf::from(explicit));
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().expect("workspace root").to_path_buf();
    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace_root.join("target"));

    let release_binary = target_dir.join("release").join("castra-vizier");
    if release_binary.is_file() {
        return Some(release_binary);
    }

    let debug_binary = target_dir.join("debug").join("castra-vizier");
    if debug_binary.is_file() {
        return Some(debug_binary);
    }

    None
}
