use crate::{error::Error, utils};

#[test]
fn to_sanitized_segments_basic() {
    let segments =
        utils::to_sanitized_segments("Parent Col 1/Child Col 1/Grand Child Col 1").unwrap();

    assert_eq!(segments.len(), 3);

    assert_eq!(segments[0].name, "Parent Col 1");
    assert_eq!(segments[0].fsname, "parent-col-1");

    assert_eq!(segments[1].name, "Child Col 1");
    assert_eq!(segments[1].fsname, "child-col-1");

    assert_eq!(segments[2].name, "Grand Child Col 1");
    assert_eq!(segments[2].fsname, "grand-child-col-1");
}

#[test]
fn to_sanitized_segments_empty_relpath() {
    let segments = utils::to_sanitized_segments("   ").unwrap();
    assert!(segments.is_empty());
}

#[test]
fn to_sanitized_segments_with_whitespace_segments() {
    let segments = utils::to_sanitized_segments("  /Whitespace Child  Col 1       /   ").unwrap();

    assert_eq!(segments.len(), 1);

    assert_eq!(segments[0].name, "Whitespace Child  Col 1");
    assert_eq!(segments[0].fsname, "whitespace-child-col-1");
}

#[test]
fn to_sanitized_segments_with_multiple_slashes() {
    let segments = utils::to_sanitized_segments("Multiple Slash Col 1///Slash  Col 1").unwrap();

    assert_eq!(segments.len(), 2);

    assert_eq!(segments[0].name, "Multiple Slash Col 1");
    assert_eq!(segments[0].fsname, "multiple-slash-col-1");

    assert_eq!(segments[1].name, "Slash  Col 1");
    assert_eq!(segments[1].fsname, "slash-col-1");
}

#[test]
fn to_sanitized_segments_with_only_empty_segments() {
    let segments = utils::to_sanitized_segments("   /   /   ").unwrap();
    assert!(segments.is_empty());
}

#[test]
fn to_sanitized_segments_special_characters() {
    let segments =
        utils::to_sanitized_segments("Special@Chars Col 1/Unicode# Col 2/🔥 Emoji Col 3").unwrap();

    assert_eq!(segments.len(), 3);

    assert_eq!(segments[0].name, "Special@Chars Col 1");
    assert_eq!(segments[0].fsname, "special-chars-col-1");

    assert_eq!(segments[1].name, "Unicode# Col 2");
    assert_eq!(segments[1].fsname, "unicode-col-2");

    assert_eq!(segments[2].name, "🔥 Emoji Col 3");
    assert_eq!(segments[2].fsname, "emoji-col-3");
}

#[test]
fn to_sanitized_segments_unicode() {
    let segments = utils::to_sanitized_segments("ザク Unicode Col 1/設定 Unicode Col 2").unwrap();

    assert_eq!(segments.len(), 2);

    assert_eq!(segments[0].name, "ザク Unicode Col 1");
    assert_eq!(segments[0].fsname, "ザク-unicode-col-1");

    assert_eq!(segments[1].name, "設定 Unicode Col 2");
    assert_eq!(segments[1].fsname, "設定-unicode-col-2");
}

#[test]
fn to_sanitized_segments_trailing_slash() {
    let segments =
        utils::to_sanitized_segments("Parent Col 1/Child Trailing Slash Col 2/").unwrap();

    assert_eq!(segments.len(), 2);

    assert_eq!(segments[0].name, "Parent Col 1");
    assert_eq!(segments[0].fsname, "parent-col-1");

    assert_eq!(segments[1].name, "Child Trailing Slash Col 2");
    assert_eq!(segments[1].fsname, "child-trailing-slash-col-2");
}

#[test]
fn to_sanitized_segments_invalid_characters() {
    let segments = utils::to_sanitized_segments(
        r#"Parent|Invalid Chars Col 1/Child Col::2"/<Grand>?Child:Invalid*Chars::\Col""3"#,
    )
    .unwrap();

    assert_eq!(segments.len(), 3);

    assert_eq!(segments[0].name, "Parent|Invalid Chars Col 1");
    assert_eq!(segments[0].fsname, "parent-invalid-chars-col-1");

    assert_eq!(segments[1].name, r#"Child Col::2""#);
    assert_eq!(segments[1].fsname, "child-col-2");

    assert_eq!(segments[2].name, r#"<Grand>?Child:Invalid*Chars::-Col""3"#);
    assert_eq!(segments[2].fsname, "grand-child-invalid-chars-col-3");
}

#[test]
fn to_sanitized_segments_reserved_names_should_be_handled() {
    let result = utils::to_sanitized_segments("NUL/Child Col 1");

    match result {
        Err(Error::SanitizationError(msg)) => {
            assert!(msg.contains("nul"), "Error should mention reserved name");
        }
        _ => panic!("Expected SanitizationError for reserved name"),
    }
}
