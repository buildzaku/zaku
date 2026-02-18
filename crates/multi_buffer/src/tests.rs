use gpui::{App, AppContext};
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
