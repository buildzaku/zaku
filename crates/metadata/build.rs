use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Deserialize)]
struct Manifest {
    package: Package,
}

#[derive(Deserialize)]
struct Package {
    description: String,
    metadata: PackageMetadata,
}

#[derive(Deserialize)]
struct PackageMetadata {
    bundle: BundleMetadata,
}

#[derive(Deserialize)]
struct BundleMetadata {
    name: String,
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
    let package = manifest_package(&manifest_path);
    let commit_sha = commit_sha();

    println!("cargo:rerun-if-changed={}", manifest_path.display());
    println!("cargo:rerun-if-changed={}", git_log_path.display());
    println!("cargo:rustc-env=ZAKU_NAME={}", package.metadata.bundle.name);
    println!("cargo:rustc-env=ZAKU_DESCRIPTION={}", package.description);
    println!(
        "cargo:rustc-env=ZAKU_IDENTIFIER={}",
        package.metadata.bundle.identifier
    );
    if let Some(build_id) = option_env!("GITHUB_RUN_NUMBER") {
        println!("cargo:rustc-env=ZAKU_BUILD_ID={build_id}");
    }
    println!("cargo:rustc-env=ZAKU_COMMIT_SHA={commit_sha}");
}

fn manifest_package(path: &Path) -> Package {
    let content = fs::read_to_string(path).expect("failed to read manifest file");
    let manifest: Manifest = toml::from_str(&content).expect("failed to parse manifest file");

    manifest.package
}

fn commit_sha() -> String {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .expect("failed to run `git rev-parse --short HEAD`");

    assert!(
        output.status.success(),
        "`git rev-parse --short HEAD` failed with status {}",
        output.status
    );

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}
