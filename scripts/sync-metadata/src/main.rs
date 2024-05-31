use serde::Deserialize;
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

    let package_json_content = fs::read_to_string(package_json_path).unwrap();
    let package_json: PackageJson = serde_json::from_str(&package_json_content).unwrap();

    let cargo_toml_content = fs::read_to_string(&cargo_toml_path).unwrap();
    let mut cargo_toml: DocumentMut = cargo_toml_content.parse::<DocumentMut>().unwrap();

    cargo_toml["package"]["name"] = value(package_json.name);
    cargo_toml["package"]["version"] = value(package_json.version);
    cargo_toml["package"]["description"] = value(package_json.description);
    cargo_toml["package"]["license"] = value(package_json.license);

    let mut authors_array = Array::new();
    authors_array.push(package_json.author);
    cargo_toml["package"]["authors"] = value(authors_array);

    cargo_toml["package"]["repository"] = value(package_json.repository);
    cargo_toml["package"]["homepage"] = value(package_json.homepage);

    fs::write(&cargo_toml_path, cargo_toml.to_string()).unwrap();

    println!("Synced Cargo.toml metadata with package.json");
    Ok(())
}
