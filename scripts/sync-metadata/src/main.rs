use serde::Deserialize;
use serde_yaml::{from_str, to_string, Value};
use std::{fs, path::PathBuf};
use toml_edit::{value, Array, DocumentMut};

#[derive(Deserialize)]
struct PackageJson {
    name: String,
    version: String,
    description: String,
    license: String,
    author: String,
    repository: String,
    homepage: String,
}

fn main() -> std::io::Result<()> {
    let root_path = PathBuf::from("..").join("..");
    let pkgcfg_path = root_path.join("package.json");
    let manifest_path = root_path.join("src-tauri").join("Cargo.toml");
    let snapcfg_path = root_path.join("snapcraft.yaml");

    let pkgcfg_content = fs::read_to_string(pkgcfg_path).unwrap();
    let pkgcfg: PackageJson = serde_json::from_str(&pkgcfg_content).unwrap();

    let manifest_content = fs::read_to_string(&manifest_path).unwrap();
    let mut manifest: DocumentMut = manifest_content.parse::<DocumentMut>().unwrap();

    manifest["package"]["name"] = value(pkgcfg.name.clone());
    manifest["package"]["version"] = value(pkgcfg.version.clone());
    manifest["package"]["description"] = value(pkgcfg.description.clone());
    manifest["package"]["license"] = value(pkgcfg.license.clone());

    let mut authors_array = Array::new();
    authors_array.push(pkgcfg.author.clone());
    manifest["package"]["authors"] = value(authors_array);

    manifest["package"]["repository"] = value(pkgcfg.repository.clone());
    manifest["package"]["homepage"] = value(pkgcfg.homepage.clone());

    fs::write(&manifest_path, manifest.to_string()).unwrap();

    let snapcfg_content = fs::read_to_string(&snapcfg_path).unwrap();
    let mut snapcfg: Value = from_str(&snapcfg_content).unwrap();

    snapcfg["name"] = Value::String(pkgcfg.name.clone());
    snapcfg["version"] = Value::String(pkgcfg.version.clone());
    snapcfg["summary"] = Value::String(pkgcfg.description.clone());

    let snapcfg_string = to_string(&snapcfg).unwrap();
    fs::write(&snapcfg_path, snapcfg_string).unwrap();

    println!("Synced Cargo.toml and snapcraft.yaml metadata with package.json");

    Ok(())
}
