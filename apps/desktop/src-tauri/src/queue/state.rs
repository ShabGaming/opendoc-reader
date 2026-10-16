//! Which actions are allowed in which queue state (docs/protocol.md, section 2.2 and 2.6).
//!
//! ```text
//! waiting --inspect (PDF with text)--> needsAttention --set text mode--> waiting
//! waiting --start--> converting --pause--> paused --resume--> converting
//! converting --> done | failed | cancelled
//! waiting | needsAttention | paused --cancel--> cancelled
//! failed | cancelled --retry--> waiting
//! ```
//!
//! After an app restart nothing is converting: `converting` and `paused` go back to `waiting`.

use crate::contract::QueueStatus;
use QueueStatus::{Cancelled, Converting, Done, Failed, NeedsAttention, Paused, Waiting};

pub fn can_start(status: QueueStatus) -> bool {
    matches!(status, Waiting)
}

pub fn can_cancel(status: QueueStatus) -> bool {
    matches!(status, Waiting | Converting | Paused | NeedsAttention)
}

pub fn can_retry(status: QueueStatus) -> bool {
    matches!(status, Failed | Cancelled)
}

pub fn can_remove(status: QueueStatus) -> bool {
    !matches!(status, Converting)
}

pub fn can_set_text_mode(status: QueueStatus) -> bool {
    matches!(status, NeedsAttention)
}

/// States the runner may pick up, paused items first (see [`pick_order`]).
pub fn is_runnable(status: QueueStatus, has_model: bool) -> bool {
    match status {
        Paused => true,
        Waiting => has_model,
        _ => false,
    }
}

/// Paused items resume before waiting ones start.
pub fn pick_order(status: QueueStatus) -> u8 {
    if status == Paused {
        0
    } else {
        1
    }
}

pub fn after_restart(status: QueueStatus) -> QueueStatus {
    match status {
        Converting | Paused => Waiting,
        other => other,
    }
}

pub fn is_finished(status: QueueStatus) -> bool {
    matches!(status, Done | Cancelled)
}

/// States in which an item is part of the live queue (a path in one of them is not added twice).
pub fn is_active(status: QueueStatus) -> bool {
    matches!(status, Waiting | Converting | Paused | NeedsAttention)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [QueueStatus; 7] = [
        Waiting,
        Converting,
        Paused,
        NeedsAttention,
        Done,
        Failed,
        Cancelled,
    ];

    fn allowed(check: fn(QueueStatus) -> bool) -> Vec<QueueStatus> {
        ALL.into_iter().filter(|s| check(*s)).collect()
    }

    #[test]
    fn only_waiting_items_can_be_started() {
        assert_eq!(allowed(can_start), vec![Waiting]);
    }

    #[test]
    fn live_items_can_be_cancelled_and_finished_ones_cannot() {
        assert_eq!(
            allowed(can_cancel),
            vec![Waiting, Converting, Paused, NeedsAttention]
        );
    }

    #[test]
    fn failed_and_cancelled_items_can_be_retried() {
        assert_eq!(allowed(can_retry), vec![Failed, Cancelled]);
    }

    #[test]
    fn everything_but_a_converting_item_can_be_removed() {
        assert!(!can_remove(Converting));
        assert_eq!(allowed(can_remove).len(), 6);
    }

    #[test]
    fn only_a_pending_text_question_accepts_a_text_mode() {
        assert_eq!(allowed(can_set_text_mode), vec![NeedsAttention]);
    }

    #[test]
    fn the_runner_takes_paused_items_and_stamped_waiting_items() {
        assert!(is_runnable(Paused, false));
        assert!(is_runnable(Waiting, true));
        assert!(!is_runnable(Waiting, false));
        for status in [Converting, NeedsAttention, Done, Failed, Cancelled] {
            assert!(!is_runnable(status, true), "{status:?}");
        }
        assert!(pick_order(Paused) < pick_order(Waiting));
    }

    #[test]
    fn a_restart_returns_interrupted_items_to_waiting() {
        assert_eq!(after_restart(Converting), Waiting);
        assert_eq!(after_restart(Paused), Waiting);
        for status in [Waiting, NeedsAttention, Done, Failed, Cancelled] {
            assert_eq!(after_restart(status), status);
        }
    }

    #[test]
    fn clear_done_removes_done_and_cancelled_only() {
        assert_eq!(
            ALL.into_iter()
                .filter(|s| is_finished(*s))
                .collect::<Vec<_>>(),
            vec![Done, Cancelled]
        );
    }
}
