fn main() {
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rerun-if-env-changed=GITHUB_RUN_NUMBER");
        resources::windows::compile(true).expect("failed to compile Windows resources");
    }
}
