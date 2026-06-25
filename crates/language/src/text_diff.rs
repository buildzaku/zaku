use imara_diff::{Algorithm, Diff as ImaraDiff, InternedInput, Token};
use std::{ops::Range, sync::Arc};

pub(crate) fn text_diff(old_text: &str, new_text: &str) -> Vec<(Range<usize>, Arc<str>)> {
    let empty: Arc<str> = Arc::default();
    let mut edits = Vec::new();
    let input = InternedInput::new(old_text, new_text);

    diff_internal(&input, &mut |old_byte_range, new_byte_range| {
        let replacement_text = if new_byte_range.is_empty() {
            empty.clone()
        } else {
            new_text
                .get(new_byte_range)
                .expect("diff replacement range should be valid")
                .into()
        };
        edits.push((old_byte_range, replacement_text));
    });

    edits
}

fn diff_internal(
    input: &InternedInput<&str>,
    on_change: &mut dyn FnMut(Range<usize>, Range<usize>),
) {
    let mut old_offset = 0;
    let mut new_offset = 0;
    let mut old_token_index = 0;
    let mut new_token_index = 0;
    let diff = ImaraDiff::compute(Algorithm::Histogram, input);

    for hunk in diff.hunks() {
        let Some(old_start) = usize::try_from(hunk.before.start).ok() else {
            return;
        };
        let Some(old_end) = usize::try_from(hunk.before.end).ok() else {
            return;
        };
        let Some(new_start) = usize::try_from(hunk.after.start).ok() else {
            return;
        };
        let Some(new_end) = usize::try_from(hunk.after.end).ok() else {
            return;
        };

        old_offset += token_len(
            input,
            input
                .before
                .get(old_token_index..old_start)
                .expect("diff token range should be valid"),
        );
        new_offset += token_len(
            input,
            input
                .after
                .get(new_token_index..new_start)
                .expect("diff token range should be valid"),
        );
        let old_len = token_len(
            input,
            input
                .before
                .get(old_start..old_end)
                .expect("diff token range should be valid"),
        );
        let new_len = token_len(
            input,
            input
                .after
                .get(new_start..new_end)
                .expect("diff token range should be valid"),
        );
        let old_byte_range = old_offset..old_offset + old_len;
        let new_byte_range = new_offset..new_offset + new_len;
        old_token_index = old_end;
        new_token_index = new_end;
        old_offset = old_byte_range.end;
        new_offset = new_byte_range.end;
        on_change(old_byte_range, new_byte_range);
    }
}

fn token_len(input: &InternedInput<&str>, tokens: &[Token]) -> usize {
    tokens
        .iter()
        .map(|token| input.interner[*token].len())
        .sum()
}
