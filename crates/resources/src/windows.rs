use indoc::formatdoc;
use semver::Version;
use std::{
    env, error,
    path::{Path, PathBuf},
};

pub fn compile(manifest: bool) -> Result<(), Box<dyn error::Error>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let icon_path = manifest_dir.join("../zaku/resources/windows/app-icon.ico");
    let manifest_path = manifest_dir.join("windows/manifest.xml");
    let package_version = env::var("CARGO_PKG_VERSION")?;
    let version = Version::parse(&package_version)?;
    let mut display_version = if version.patch == 0 {
        format!("{}.{}", version.major, version.minor)
    } else {
        format!("{}.{}.{}", version.major, version.minor, version.patch)
    };
    if !version.pre.is_empty() {
        display_version.push('-');
        display_version.push_str(version.pre.as_str());
    }
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
                    VALUE "FileVersion", "{display_version}\0"
                    VALUE "ProductName", "Zaku\0"
                    VALUE "ProductVersion", "{display_version}\0"
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

    std::fs::write(&resource_path, resource)?;
    embed_resource::compile(&resource_path, embed_resource::NONE).manifest_required()?;

    Ok(())
}

fn escape_resource_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}
