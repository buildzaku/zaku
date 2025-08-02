pub fn is_string_none_or_empty(value: &Option<String>) -> bool {
    value.as_ref().is_none_or(|s| s.is_empty())
}
