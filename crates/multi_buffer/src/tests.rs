use gpui::{App, AppContext};
use indoc::indoc;
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
