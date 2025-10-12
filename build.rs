use std::env;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");

    let pkg_version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string());
    let git_sha = git_short_sha();
    let version_string = git_sha
        .as_ref()
        .map(|sha| format!("{pkg_version} ({sha})"))
        .unwrap_or_else(|| pkg_version.clone());

    println!("cargo:rustc-env=CASTRA_VERSION={}", version_string);
    if let Some(sha) = git_sha {
        println!("cargo:rustc-env=CASTRA_GIT_SHA={}", sha);
    } else {
        println!("cargo:rustc-env=CASTRA_GIT_SHA=");
    }
}

fn git_short_sha() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if sha.is_empty() { None } else { Some(sha) }
}
