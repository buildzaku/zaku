pub fn join_str_paths(paths: Vec<&str>) -> String {
    return paths
        .into_iter()
        .filter(|path| !path.is_empty())
        .collect::<Vec<&str>>()
        .join("/");
}
