use indoc::formatdoc;
use semver::Version;
use std::{
    env, error,
    path::{Path, PathBuf},
    process::Command,
};

fn commit_sha() -> Option<String> {
    if let Ok(commit_sha) = env::var("ZAKU_COMMIT_SHA")
        && !commit_sha.is_empty()
    {
        return Some(commit_sha);
    }

    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let commit_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!commit_sha.is_empty()).then_some(commit_sha)
}

fn product_version(package_version: &str) -> String {
    let mut metadata = Vec::new();
    if let Ok(build_id) = env::var("GITHUB_RUN_NUMBER")
        && !build_id.is_empty()
    {
        metadata.push(build_id);
    }
    if let Some(commit_sha) = commit_sha() {
        metadata.push(commit_sha);
    }

    if metadata.is_empty() {
        package_version.to_string()
    } else {
        format!("{package_version}+{}", metadata.join("."))
    }
}

pub fn compile(manifest: bool) -> Result<(), Box<dyn error::Error>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let git_log_path = manifest_dir.join("../../.git/logs/HEAD");
    let icon_path = manifest_dir.join("../zaku/resources/windows/app-icon.ico");
    let manifest_path = manifest_dir.join("windows/manifest.xml");
    let package_version = env::var("CARGO_PKG_VERSION")?;
    let product_version = product_version(&package_version);
    let version = Version::parse(&package_version)?;
    let major = u16::try_from(version.major)?;
    let minor = u16::try_from(version.minor)?;
    let patch = u16::try_from(version.patch)?;
    let file_version = format!("{major},{minor},{patch},0");
    let escaped_icon_path = escape_resource_path(&icon_path);
    let manifest_line = if manifest {
        let escaped_manifest_path = escape_resource_path(&manifest_path);
        format!(r#"1 24 "{escaped_manifest_path}""#)
    } else {
        String::new()
    };
    let resource = formatdoc! {r#"
        1 ICON "{escaped_icon_path}"
        {manifest_line}

        1 VERSIONINFO
        FILEVERSION {file_version}
        PRODUCTVERSION {file_version}
        FILEFLAGSMASK 0x3fL
        FILEFLAGS 0x0L
        FILEOS 0x40004L
        FILETYPE 0x1L
        FILESUBTYPE 0x0L
        BEGIN
            BLOCK "StringFileInfo"
            BEGIN
                BLOCK "040904b0"
                BEGIN
                    VALUE "FileDescription", "Zaku\0"
                    VALUE "FileVersion", "{package_version}\0"
                    VALUE "ProductName", "Zaku\0"
                    VALUE "ProductVersion", "{product_version}\0"
                END
            END
            BLOCK "VarFileInfo"
            BEGIN
                VALUE "Translation", 0x0409, 1200
            END
        END
    "#};
    let resource_path = PathBuf::from(env::var("OUT_DIR")?).join("zaku_resources.rc");

    println!("cargo:rerun-if-changed={}", icon_path.display());
    println!("cargo:rerun-if-changed={}", manifest_path.display());
    println!("cargo:rerun-if-changed={}", git_log_path.display());

    std::fs::write(&resource_path, resource)?;
    embed_resource::compile(&resource_path, embed_resource::NONE).manifest_required()?;

    Ok(())
}

fn escape_resource_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}
