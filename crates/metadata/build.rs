use std::{path::PathBuf, process::Command};

use resources::ZakuManifest;

fn main() {
    let git_log_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(".git")
        .join("logs")
        .join("HEAD");
    let package = ZakuManifest::load()
        .expect("failed to read manifest file")
        .package;
    let commit_sha = commit_sha();

    println!("cargo:rerun-if-changed={}", ZakuManifest::PATH);
    println!("cargo:rerun-if-changed={}", git_log_path.display());
    println!("cargo:rustc-env=ZAKU_NAME={}", package.metadata.bundle.name);
    println!("cargo:rustc-env=ZAKU_DESCRIPTION={}", package.description);
    println!("cargo:rustc-env=ZAKU_VERSION={}", package.version);
    println!(
        "cargo:rustc-env=ZAKU_IDENTIFIER={}",
        package.metadata.bundle.identifier
    );
    if let Some(build_id) = option_env!("GITHUB_RUN_NUMBER") {
        println!("cargo:rustc-env=ZAKU_BUILD_ID={build_id}");
    }
    println!("cargo:rustc-env=ZAKU_COMMIT_SHA={commit_sha}");
}

fn commit_sha() -> String {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("failed to run `git rev-parse HEAD`");

    assert!(
        output.status.success(),
        "`git rev-parse HEAD` failed with status {}",
        output.status
    );

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}
