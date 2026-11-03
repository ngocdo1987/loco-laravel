pub use crate::models::_entities::comments::{self, ActiveModel, Entity, Model};

/// Comment status values (goravel `CommentStatusPending` /
/// `CommentStatusApproved` / `CommentStatusRejected`).
pub const STATUS_PENDING: &str = "pending";
pub const STATUS_APPROVED: &str = "approved";
pub const STATUS_REJECTED: &str = "rejected";

impl Model {
    /// True while the comment awaits moderation (not publicly visible).
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.status == STATUS_PENDING
    }

    /// True when the comment passed moderation (publicly visible).
    #[must_use]
    pub fn is_approved(&self) -> bool {
        self.status == STATUS_APPROVED
    }

    /// True when the comment was rejected during moderation.
    #[must_use]
    pub fn is_rejected(&self) -> bool {
        self.status == STATUS_REJECTED
    }
}
