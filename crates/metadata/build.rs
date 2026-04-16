use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Deserialize)]
struct BundleMetadata {
    name: String,
    description: String,
    version: String,
    repository: String,
    identifier: String,
}

fn main() {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("zaku")
        .join("Cargo.toml");
    let git_log_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(".git")
        .join("logs")
        .join("HEAD");
    let metadata = manifest_metadata(&manifest_path);
    let commit_sha = commit_sha();

    println!("cargo:rerun-if-changed={}", manifest_path.display());
    println!("cargo:rerun-if-changed={}", git_log_path.display());
    println!("cargo:rustc-env=ZAKU_NAME={}", metadata.name);
    println!("cargo:rustc-env=ZAKU_DESCRIPTION={}", metadata.description);
    println!("cargo:rustc-env=ZAKU_VERSION={}", metadata.version);
    println!("cargo:rustc-env=ZAKU_IDENTIFIER={}", metadata.identifier);
    println!("cargo:rustc-env=ZAKU_REPOSITORY={}", metadata.repository);
    println!("cargo:rustc-env=ZAKU_COMMIT_SHA={commit_sha}");
}

fn manifest_metadata(path: &Path) -> BundleMetadata {
    let manifest = fs::read_to_string(path).unwrap_or_else(|error| {
        panic!(
            "Failed to read package manifest at {}: {error}",
            path.display()
        )
    });
    let manifest: toml::Value = toml::from_str(&manifest).unwrap_or_else(|error| {
        panic!(
            "Failed to parse package manifest at {}: {error}",
            path.display()
        )
    });

    manifest["package"]["metadata"]["bundle"]
        .clone()
        .try_into()
        .unwrap_or_else(|error| {
            panic!(
                "Failed to parse [package.metadata.bundle] in {}: {error}",
                path.display()
            )
        })
}

fn commit_sha() -> String {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .expect("Failed to run `git rev-parse --short HEAD`");

    assert!(
        output.status.success(),
        "`git rev-parse --short HEAD` failed with status {}",
        output.status
    );

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}
