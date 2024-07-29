use serde::Deserialize;
use serde_yaml::{from_str, to_string, Value};
use std::fs;
use std::path::PathBuf;
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
    let package_json_path = PathBuf::from("./../../package.json");
    let cargo_toml_path = PathBuf::from("./../../src-tauri/Cargo.toml");
    let snapcraft_yaml_path = PathBuf::from("./../../snapcraft.yaml");

    let package_json_content = fs::read_to_string(package_json_path).unwrap();
    let package_json: PackageJson = serde_json::from_str(&package_json_content).unwrap();

    let cargo_toml_content = fs::read_to_string(&cargo_toml_path).unwrap();
    let mut cargo_toml: DocumentMut = cargo_toml_content.parse::<DocumentMut>().unwrap();

    cargo_toml["package"]["name"] = value(package_json.name.clone());
    cargo_toml["package"]["version"] = value(package_json.version.clone());
    cargo_toml["package"]["description"] = value(package_json.description.clone());
    cargo_toml["package"]["license"] = value(package_json.license.clone());

    let mut authors_array = Array::new();
    authors_array.push(package_json.author.clone());
    cargo_toml["package"]["authors"] = value(authors_array);

    cargo_toml["package"]["repository"] = value(package_json.repository.clone());
    cargo_toml["package"]["homepage"] = value(package_json.homepage.clone());

    fs::write(&cargo_toml_path, cargo_toml.to_string()).unwrap();

    let snapcraft_yaml_content = fs::read_to_string(&snapcraft_yaml_path).unwrap();
    let mut snapcraft_yaml: Value = from_str(&snapcraft_yaml_content).unwrap();

    snapcraft_yaml["name"] = Value::String(package_json.name.clone());
    snapcraft_yaml["version"] = Value::String(package_json.version.clone());
    snapcraft_yaml["summary"] = Value::String(package_json.description.clone());

    let snapcraft_yaml_string = to_string(&snapcraft_yaml).unwrap();
    fs::write(&snapcraft_yaml_path, snapcraft_yaml_string).unwrap();

    println!("Synced Cargo.toml and snapcraft.yaml metadata with package.json");
    Ok(())
}
