use gpui::Context;
use std::time::Instant;
use text::TransactionId;

use crate::MultiBuffer;

impl MultiBuffer {
    pub fn start_transaction_at(
        &mut self,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Option<TransactionId> {
        if let Some(buffer) = self.as_singleton() {
            return buffer.update(cx, |buffer, _| buffer.start_transaction_at(now));
        }

        None
    }

    pub fn end_transaction_at(
        &mut self,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Option<TransactionId> {
        if let Some(buffer) = self.as_singleton() {
            return buffer.update(cx, |buffer, _| {
                buffer
                    .end_transaction_at(now)
                    .map(|(transaction_id, _)| transaction_id)
            });
        }

        None
    }

    pub fn undo(&mut self, cx: &mut Context<Self>) -> Option<TransactionId> {
        let mut transaction_id = None;
        if let Some(buffer) = self.as_singleton() {
            transaction_id = buffer.update(cx, |buffer, _| {
                buffer.undo().map(|(transaction_id, _)| transaction_id)
            });
        }

        transaction_id
    }

    pub fn redo(&mut self, cx: &mut Context<Self>) -> Option<TransactionId> {
        if let Some(buffer) = self.as_singleton() {
            return buffer.update(cx, |buffer, _| {
                buffer.redo().map(|(transaction_id, _)| transaction_id)
            });
        }

        None
    }
}
