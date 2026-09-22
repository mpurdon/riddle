//! riddle-activity: the Activity ledger helpers, the sentence formatter and
//! the undo engine (ARCHITECTURE.md §47, §50). Activity answers "what
//! happened, who did it, why, can I undo it"; this crate makes every
//! producer write the same row and makes undo honest about what it can and
//! cannot restore.

pub mod actions;
pub mod format;
pub mod ledger;
pub mod undo;

pub use actions::{actions, RuntimeHandle};
pub use format::{actor_name, disposition_name, find_type_name, sentence, undo_sentence, Sentence};
pub use ledger::{Entry, Ledger};
pub use undo::{NoProviderCheck, ProviderStateCheck, UndoEngine, ALL_OR_NOTHING, HANDLED_SINCE};
