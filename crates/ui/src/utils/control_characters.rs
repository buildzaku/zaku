use std::borrow::Cow;

const LINE_FEED_SYMBOL: char = '↵';
const CONTROL_PICTURES_START: u32 = 0x2400;
const DELETE_SYMBOL: char = '␡';

// C1 controls have no Control Pictures equivalent, so they are left unchanged.
fn printable_substitute(character: char) -> Option<char> {
    match character {
        '\n' => Some(LINE_FEED_SYMBOL),
        '\x7f' => Some(DELETE_SYMBOL),
        character if character.is_ascii_control() => {
            char::from_u32(CONTROL_PICTURES_START + u32::from(character))
        }
        _ => None,
    }
}

pub fn replace_control_characters(text: &str) -> Cow<'_, str> {
    let Some(first_control_character) = text.find(|character: char| character.is_ascii_control())
    else {
        return Cow::Borrowed(text);
    };

    let (printable_prefix, rest) = text.split_at(first_control_character);
    let mut replaced = String::with_capacity(text.len());
    replaced.push_str(printable_prefix);
    for character in rest.chars() {
        match printable_substitute(character) {
            Some(substitute) => replaced.push(substitute),
            None => replaced.push(character),
        }
    }

    Cow::Owned(replaced)
}

pub fn replace_control_characters_remapping_offsets(
    text: &str,
    offsets: &mut [usize],
) -> Option<String> {
    // ASCII control bytes cannot be mistaken for UTF-8 continuation bytes.
    if !text.bytes().any(|byte| byte.is_ascii_control()) {
        return None;
    }

    // Replacements occupy more bytes, so callers' offsets must move with the text.
    // Only character boundaries are mapped; callers must supply boundary offsets.
    let mut moved = vec![0; text.len() + 1];
    let mut replaced = String::with_capacity(text.len());
    for (offset, character) in text.char_indices() {
        *moved
            .get_mut(offset)
            .expect("character offset should be in bounds") = replaced.len();
        match printable_substitute(character) {
            Some(substitute) => replaced.push(substitute),
            None => replaced.push(character),
        }
    }
    *moved
        .get_mut(text.len())
        .expect("end offset should be in bounds") = replaced.len();

    for offset in offsets {
        debug_assert!(
            *offset > text.len() || text.is_char_boundary(*offset),
            "offset {offset} is not on a character boundary of {text:?}",
        );
        *offset = moved.get(*offset).copied().unwrap_or(replaced.len());
    }

    Some(replaced)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_printable_text_is_not_reallocated() {
        assert!(matches!(
            replace_control_characters("main.rs"),
            Cow::Borrowed("main.rs")
        ));
        assert!(matches!(replace_control_characters(""), Cow::Borrowed("")));
    }

    #[test]
    fn test_replaces_control_characters() {
        assert_eq!(replace_control_characters("a\nb"), "a↵b");
        assert_eq!(replace_control_characters("a\rb"), "a␍b");
        assert_eq!(replace_control_characters("a\tb"), "a␉b");
        assert_eq!(replace_control_characters("a\0b"), "a␀b");
        assert_eq!(replace_control_characters("a\x7fb"), "a␡b");
    }

    #[test]
    fn test_replaces_every_occurrence() {
        assert_eq!(replace_control_characters("\r\n\t"), "␍↵␉");
        assert_eq!(replace_control_characters("a\nb\nc"), "a↵b↵c");
    }

    #[test]
    fn test_preserves_non_control_characters() {
        assert_eq!(replace_control_characters("é\nü"), "é↵ü");
        assert_eq!(replace_control_characters("🚀"), "🚀");
        assert_eq!(replace_control_characters("🚀\t🚀"), "🚀␉🚀");
    }

    #[test]
    fn test_character_count_is_preserved() {
        for text in ["a\nb", "\r\n\t", "a\x7fb", "é\nü"] {
            assert_eq!(
                replace_control_characters(text).chars().count(),
                text.chars().count(),
            );
        }
    }

    #[test]
    fn test_leaves_c1_control_characters_alone() {
        assert_eq!(replace_control_characters("a\u{0085}b"), "a\u{0085}b");
    }

    #[test]
    fn test_printable_text_is_left_alone_when_remapping() {
        let mut offsets = vec![0, 3];
        assert_eq!(
            replace_control_characters_remapping_offsets("main.rs", &mut offsets),
            None,
        );
        assert_eq!(offsets, vec![0, 3], "offsets must not move");
    }

    #[test]
    fn test_remaps_offsets_after_a_replacement() {
        let mut offsets = vec![0, 1, 2];
        let replaced = replace_control_characters_remapping_offsets("a\tb", &mut offsets).unwrap();
        assert_eq!(replaced, "a␉b");
        assert_eq!(offsets, vec![0, 1, 4]);
    }

    #[test]
    fn test_remaps_offsets_across_several_replacements() {
        let mut offsets = vec![0, 2, 4];
        let replaced =
            replace_control_characters_remapping_offsets("a\tb\tc", &mut offsets).unwrap();
        assert_eq!(replaced, "a␉b␉c");
        assert_eq!(offsets, vec![0, 4, 8]);
    }

    #[test]
    fn test_remaps_offsets_around_multi_byte_characters() {
        let mut offsets = vec![0, 2, 5];
        let replaced = replace_control_characters_remapping_offsets("é\tü", &mut offsets).unwrap();
        assert_eq!(replaced, "é␉ü");
        assert_eq!(offsets, vec![0, 2, 5 + 2]);
    }

    #[test]
    fn test_remapped_offsets_stay_on_character_boundaries() {
        for text in ["a\nb", "\r\n\t", "é\nü", "🚀\t🚀", "a\x7fb"] {
            let mut offsets: Vec<usize> = text.char_indices().map(|(offset, _)| offset).collect();
            let replaced =
                replace_control_characters_remapping_offsets(text, &mut offsets).unwrap();
            for offset in offsets {
                assert!(
                    replaced.is_char_boundary(offset),
                    "offset {offset} is not a boundary of {replaced:?} (from {text:?})",
                );
            }
        }
    }

    #[test]
    fn test_remaps_the_end_offset() {
        let mut offsets = vec!["a\tb".len()];
        let replaced = replace_control_characters_remapping_offsets("a\tb", &mut offsets).unwrap();
        assert_eq!(offsets, vec![replaced.len()]);
    }

    #[test]
    fn test_clamps_offsets_past_the_end() {
        let mut offsets = vec![999];
        let replaced = replace_control_characters_remapping_offsets("a\tb", &mut offsets).unwrap();
        assert_eq!(offsets, vec![replaced.len()]);
    }

    #[test]
    fn test_remapping_agrees_with_the_borrowing_variant() {
        for text in ["main.rs", "a\nb", "\r\n\t", "é\nü", "🚀\t🚀"] {
            let remapped = replace_control_characters_remapping_offsets(text, &mut []);
            let borrowed = replace_control_characters(text);
            match remapped {
                Some(replaced) => assert_eq!(replaced, borrowed.as_ref()),
                None => assert!(matches!(borrowed, Cow::Borrowed(_))),
            }
        }
    }
}
