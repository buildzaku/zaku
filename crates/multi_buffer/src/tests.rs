use gpui::{App, AppContext};
use indoc::indoc;
use rand::{RngExt, rngs::StdRng};
use std::time::{Duration, Instant};
use text::{Buffer as TextBuffer, BufferId, Point, ReplicaId};

use super::*;

#[gpui::test]
fn test_empty_singleton(cx: &mut App) {
    let buffer_id = BufferId::new(1).unwrap();
    let buffer = cx.new(|_| TextBuffer::new(ReplicaId::LOCAL, buffer_id, ""));
    assert_eq!(buffer.read(cx).remote_id(), buffer_id);

    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let snapshot = multibuffer.read(cx).snapshot(cx);

    assert_eq!(snapshot.text(), "");
    assert_eq!(snapshot.len(), MultiBufferOffset::ZERO);
    assert_eq!(snapshot.max_point(), Point::zero());
}

#[gpui::test]
fn test_singleton(cx: &mut App) {
    let buffer_id = BufferId::new(1).unwrap();
    let buffer = cx.new(|_| {
        TextBuffer::new(
            ReplicaId::LOCAL,
            buffer_id,
            indoc! {"
                The quick brown fox
                jumps over the lazy dog
            "},
        )
    });
    assert_eq!(buffer.read(cx).remote_id(), buffer_id);
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));

    let snapshot = multibuffer.read(cx).snapshot(cx);
    assert_eq!(snapshot.text(), buffer.read(cx).text());

    buffer.update(cx, |buffer, _| {
        buffer.edit([(Point::new(1, 0)..Point::new(1, 5), "leaps")]);
    });

    let snapshot = multibuffer.read(cx).snapshot(cx);
    assert_eq!(
        snapshot.text(),
        indoc! {"
            The quick brown fox
            leaps over the lazy dog
        "}
    );
    assert_eq!(snapshot.text(), buffer.read(cx).text());
}

#[gpui::test]
fn test_singleton_multibuffer_anchors(cx: &mut App) {
    let buffer_id = BufferId::new(1).unwrap();
    let buffer = cx.new(|_| TextBuffer::new(ReplicaId::LOCAL, buffer_id, "abcd"));
    assert_eq!(buffer.read(cx).remote_id(), buffer_id);

    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));
    let old_snapshot = multibuffer.read(cx).snapshot(cx);

    buffer.update(cx, |buffer, _| {
        buffer.edit([(0..0, "X")]);
        buffer.edit([(5..5, "Y")]);
    });

    let new_snapshot = multibuffer.read(cx).snapshot(cx);

    assert_eq!(old_snapshot.text(), "abcd");
    assert_eq!(new_snapshot.text(), "XabcdY");

    assert_eq!(
        old_snapshot
            .anchor_before(MultiBufferOffset(0))
            .to_offset(&new_snapshot),
        MultiBufferOffset(0)
    );
    assert_eq!(
        old_snapshot
            .anchor_after(MultiBufferOffset(0))
            .to_offset(&new_snapshot),
        MultiBufferOffset(1)
    );
    assert_eq!(
        old_snapshot
            .anchor_before(MultiBufferOffset(4))
            .to_offset(&new_snapshot),
        MultiBufferOffset(5)
    );
    assert_eq!(
        old_snapshot
            .anchor_after(MultiBufferOffset(4))
            .to_offset(&new_snapshot),
        MultiBufferOffset(6)
    );
}

#[gpui::test]
fn test_trailing_deletion_without_newline(cx: &mut App) {
    let buffer_id = BufferId::new(1).unwrap();
    let buffer = cx.new(|_| {
        TextBuffer::new(
            ReplicaId::LOCAL,
            buffer_id,
            "The quick brown fox\njumps over the lazy dog",
        )
    });
    assert_eq!(buffer.read(cx).remote_id(), buffer_id);

    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.edit([(Point::new(0, 19)..Point::new(1, 23), "")], cx);
    });

    let snapshot = multibuffer.read(cx).snapshot(cx);
    assert_eq!(snapshot.text(), "The quick brown fox");
    assert_eq!(snapshot.max_point(), Point::new(0, 19));
    assert_eq!(snapshot.len(), MultiBufferOffset(19));
    assert_eq!(
        snapshot.point_to_offset(Point::new(0, 19)),
        MultiBufferOffset(19)
    );
    assert_eq!(
        snapshot.offset_to_point(MultiBufferOffset(19)),
        Point::new(0, 19)
    );
}

#[gpui::test(iterations = 100)]
fn test_random_multibuffer(cx: &mut App, mut rng: StdRng) {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz \n\t";

    let initial_len = rng.random_range(0..=128);
    let mut expected = (0..initial_len)
        .map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char)
        .collect::<String>();

    let buffer_id = BufferId::new(1).unwrap();
    let buffer = cx.new(|_| TextBuffer::new(ReplicaId::LOCAL, buffer_id, &expected));
    assert_eq!(buffer.read(cx).remote_id(), buffer_id);

    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));

    for _ in 0..10 {
        match rng.random_range(0..100) {
            0..=59 => {
                let (raw_start, raw_end, normalized_start, normalized_end) = if expected.is_empty()
                {
                    (0, 0, 0, 0)
                } else {
                    let start = rng.random_range(0..=expected.len());
                    let end = rng.random_range(0..=expected.len());
                    let (normalized_start, normalized_end) = if start <= end {
                        (start, end)
                    } else {
                        (end, start)
                    };
                    if rng.random_bool(0.3) {
                        (end, start, normalized_start, normalized_end)
                    } else {
                        (start, end, normalized_start, normalized_end)
                    }
                };
                let text_len = rng.random_range(0..=48);
                let new_text = (0..text_len)
                    .map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char)
                    .collect::<String>();

                multibuffer.update(cx, |multibuffer, cx| {
                    multibuffer.edit(
                        [(
                            MultiBufferOffset(raw_start)..MultiBufferOffset(raw_end),
                            new_text.clone(),
                        )],
                        cx,
                    );
                });

                expected.replace_range(normalized_start..normalized_end, &new_text);
            }
            60..=84 => {
                let (normalized_start, normalized_end) = if expected.is_empty() {
                    (0, 0)
                } else {
                    let start = rng.random_range(0..=expected.len());
                    let end = rng.random_range(0..=expected.len());
                    if start <= end {
                        (start, end)
                    } else {
                        (end, start)
                    }
                };
                let text_len = rng.random_range(0..=48);
                let new_text = (0..text_len)
                    .map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char)
                    .collect::<String>();

                buffer.update(cx, |buffer, _| {
                    buffer.edit([(normalized_start..normalized_end, new_text.clone())]);
                });

                expected.replace_range(normalized_start..normalized_end, &new_text);
            }
            _ => {
                let text_len = rng.random_range(0..=192);
                let new_text = (0..text_len)
                    .map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char)
                    .collect::<String>();
                multibuffer.update(cx, |multibuffer, cx| {
                    multibuffer.set_text(new_text.clone(), cx);
                });
                expected = new_text;
            }
        }

        let snapshot = multibuffer.read(cx).snapshot(cx);
        assert_eq!(snapshot.text(), expected);
        assert_eq!(buffer.read(cx).text(), expected);
        assert_eq!(snapshot.len().0, expected.len());

        for _ in 0..3 {
            let offset = MultiBufferOffset(rng.random_range(0..=snapshot.len().0));
            let point = snapshot.offset_to_point(offset);
            let offset_roundtrip = snapshot.point_to_offset(point);
            assert_eq!(offset_roundtrip, offset);

            let utf16_offset = snapshot.offset_to_offset_utf16(offset);
            assert_eq!(snapshot.offset_utf16_to_offset(utf16_offset), offset);

            let before = snapshot.anchor_before(offset);
            let after = snapshot.anchor_after(offset);
            assert!(before.to_offset(&snapshot) <= after.to_offset(&snapshot));
        }
    }
}

#[gpui::test]
fn test_history(cx: &mut App) {
    let buffer_id = BufferId::new(1).unwrap();
    let buffer = cx.new(|_| TextBuffer::new(ReplicaId::LOCAL, buffer_id, "fox"));
    assert_eq!(buffer.read(cx).remote_id(), buffer_id);

    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));
    let mut now = Instant::now();

    let first_transaction_id = multibuffer.update(cx, |multi_buffer, cx| {
        let transaction_id = multi_buffer
            .start_transaction_at(now, cx)
            .expect("first transaction should start");
        multi_buffer.edit([(MultiBufferOffset(0)..MultiBufferOffset(0), "quick ")], cx);
        multi_buffer.edit([(MultiBufferOffset(6)..MultiBufferOffset(6), "brown ")], cx);
        let ended_transaction_id = multi_buffer
            .end_transaction_at(now, cx)
            .expect("first transaction should end");
        assert_eq!(ended_transaction_id, transaction_id);
        transaction_id
    });

    assert_eq!(multibuffer.read(cx).snapshot(cx).text(), "quick brown fox");

    now += Duration::from_secs(1);
    let second_transaction_id = multibuffer.update(cx, |multi_buffer, cx| {
        let transaction_id = multi_buffer
            .start_transaction_at(now, cx)
            .expect("second transaction should start");
        multi_buffer.edit(
            [(MultiBufferOffset(15)..MultiBufferOffset(15), " jumps")],
            cx,
        );
        let ended_transaction_id = multi_buffer
            .end_transaction_at(now, cx)
            .expect("second transaction should end");
        assert_eq!(ended_transaction_id, transaction_id);
        transaction_id
    });

    assert_eq!(
        multibuffer.read(cx).snapshot(cx).text(),
        "quick brown fox jumps"
    );

    multibuffer.update(cx, |multi_buffer, cx| {
        assert_eq!(multi_buffer.undo(cx), Some(second_transaction_id));
        assert_eq!(multi_buffer.read(cx).text(), "quick brown fox");

        assert_eq!(multi_buffer.undo(cx), Some(first_transaction_id));
        assert_eq!(multi_buffer.read(cx).text(), "fox");

        assert_eq!(multi_buffer.redo(cx), Some(first_transaction_id));
        assert_eq!(multi_buffer.read(cx).text(), "quick brown fox");

        assert_eq!(multi_buffer.redo(cx), Some(second_transaction_id));
        assert_eq!(multi_buffer.read(cx).text(), "quick brown fox jumps");
    });

    now += Duration::from_secs(1);
    multibuffer.update(cx, |multi_buffer, cx| {
        assert_eq!(multi_buffer.undo(cx), Some(second_transaction_id));
        assert_eq!(multi_buffer.read(cx).text(), "quick brown fox");

        let third_transaction_id = multi_buffer
            .start_transaction_at(now, cx)
            .expect("third transaction should start");
        multi_buffer.edit([(MultiBufferOffset(0)..MultiBufferOffset(0), "The ")], cx);
        let ended_transaction_id = multi_buffer
            .end_transaction_at(now, cx)
            .expect("third transaction should end");
        assert_eq!(ended_transaction_id, third_transaction_id);
        assert_eq!(multi_buffer.read(cx).text(), "The quick brown fox");

        assert_eq!(multi_buffer.redo(cx), None);

        assert_eq!(multi_buffer.undo(cx), Some(third_transaction_id));
        assert_eq!(multi_buffer.read(cx).text(), "quick brown fox");

        assert_eq!(multi_buffer.undo(cx), Some(first_transaction_id));
        assert_eq!(multi_buffer.read(cx).text(), "fox");
    });
}
