//! Instance-owned task pipeline: state machine, runtime, preparation and inspection.
pub(crate) mod inspection;
pub(crate) mod notifications;
pub mod pipeline;
pub(crate) mod prepare;
pub(crate) mod recovery;
pub(crate) mod runtime;
pub(crate) mod timing;
