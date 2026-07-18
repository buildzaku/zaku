fn main() {
    #[cfg(target_os = "windows")]
    resources::windows::compile(false).expect("failed to compile Windows resources");
}
