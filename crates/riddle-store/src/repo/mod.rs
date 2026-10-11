//! Repositories, one per table family. Each takes a `&Store` and returns
//! domain types from `riddle-core`. Keep SQL here and nowhere else.

pub mod activities;
pub mod classifications;
pub mod conversations;
pub mod corrections;
pub mod digs;
pub mod drafts;
pub mod expeditions;
pub mod facts;
pub mod finds;
pub mod importance;
pub mod jobs;
pub mod kv;
pub mod lists;
pub mod meshes;
pub mod message_series;
pub mod messages;
pub mod outbox;
pub mod pending;
pub mod people;
pub mod provider_labels;
pub mod shadow_reviews;
pub mod signals;
pub mod strata;
pub mod sync_state;
pub mod thread_state;
pub mod traces;
pub mod usage;
pub mod user_profile;
