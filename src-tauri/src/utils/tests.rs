use crate::{error::Error, utils};
use std::path::PathBuf;

#[test]
fn to_sanitized_segments_basic() {
    let segments = utils::to_sanitized_segments(
        &PathBuf::from("Parent Col 1/Child Col 1/Grand Child Col 1"),
    )
    .unwrap();

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
    let segments = utils::to_sanitized_segments(&PathBuf::from("   ")).unwrap();
    assert!(segments.is_empty());
}

#[test]
fn to_sanitized_segments_with_whitespace_segments() {
    let segments =
        utils::to_sanitized_segments(&PathBuf::from("  /Whitespace Child    Col 1       /   "))
            .unwrap();

    assert_eq!(segments.len(), 1);

    assert_eq!(segments[0].name, "Whitespace Child Col 1");
    assert_eq!(segments[0].fsname, "whitespace-child-col-1");
}

#[test]
fn to_sanitized_segments_with_only_empty_segments() {
    let segments =
        utils::to_sanitized_segments(&PathBuf::from("   /   /   ")).unwrap();
    assert!(segments.is_empty());
}

#[test]
fn to_sanitized_segments_special_characters() {
    let segments = utils::to_sanitized_segments(
        &PathBuf::from("Special@Chars Col 1/Unicode# Col 2/🔥 Emoji Col 3"),
    )
    .unwrap();

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
    let segments = utils::to_sanitized_segments(
        &PathBuf::from("ザク Unicode Col 1/設定 Unicode Col 2"),
    )
    .unwrap();

    assert_eq!(segments.len(), 2);

    assert_eq!(segments[0].name, "ザク Unicode Col 1");
    assert_eq!(segments[0].fsname, "ザク-unicode-col-1");

    assert_eq!(segments[1].name, "設定 Unicode Col 2");
    assert_eq!(segments[1].fsname, "設定-unicode-col-2");
}

#[test]
fn to_sanitized_segments_invalid_characters() {
    let segments = utils::to_sanitized_segments(
        &PathBuf::from("Parent|Invalid/Child::Col/Grand?Child*"),
    )
    .unwrap();

    assert_eq!(segments.len(), 3);
    assert_eq!(segments[0].name, "Parent|Invalid");
    assert_eq!(segments[0].fsname, "parent-invalid");

    assert_eq!(segments[1].name, "Child::Col");
    assert_eq!(segments[1].fsname, "child-col");

    assert_eq!(segments[2].name, "Grand?Child*");
    assert_eq!(segments[2].fsname, "grand-child");
}

#[test]
fn to_sanitized_segments_reserved_names_should_be_handled() {
    let result = utils::to_sanitized_segments(&PathBuf::from("NUL/Child Col 1"));

    match result {
        Err(Error::SanitizationError(msg)) => {
            assert!(msg.contains("nul"), "Error should mention reserved name");
        }
        _ => panic!("Expected SanitizationError for reserved name"),
    }
}

#[test]
fn to_sanitized_segments_backslash() {
    let segments = utils::to_sanitized_segments(&PathBuf::from("Path\\With\\Backslashes")).unwrap();

    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].name, "Path-With-Backslashes");
    assert_eq!(segments[0].fsname, "path-with-backslashes");
}

#[test]
fn to_sanitized_segments_backslash_at_ends() {
    let segments =
        utils::to_sanitized_segments(&PathBuf::from("\\Path\\With\\Backslashes\\At\\Ends\\"))
            .unwrap();

    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].name, "Path-With-Backslashes-At-Ends");
    assert_eq!(segments[0].fsname, "path-with-backslashes-at-ends");
}

#[test]
fn to_sanitized_segments_multiple_consecutive_backslashes() {
    let segments = utils::to_sanitized_segments(&PathBuf::from(
        "\\Path\\With\\\\\\\\Multiple\\Consecutive\\\\\\\\Backslashes",
    ))
    .unwrap();

    assert_eq!(segments.len(), 1);
    assert_eq!(
        segments[0].name,
        "Path-With-Multiple-Consecutive-Backslashes"
    );
    assert_eq!(
        segments[0].fsname,
        "path-with-multiple-consecutive-backslashes"
    );
}
