fn main() {
    #[cfg(target_os = "windows")]
    resources::windows::compile(true).expect("failed to compile Windows resources");
}
