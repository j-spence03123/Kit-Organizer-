//! File organisation: planning, safe execution, recovery and undo.
//!
//! Safety rules enforced here:
//! * Nothing is ever overwritten. Final names are claimed with a
//!   no-replace primitive (hard link, falling back to check-then-rename on
//!   filesystems without hard links), and collisions get `Name (2).ext`.
//! * Copies are written to a hidden temp file in the destination folder,
//!   flushed, verified against the BLAKE3 hash recorded at scan time and
//!   only then given their final name.
//! * Moves on the same volume are atomic renames; across volumes they are a
//!   verified copy followed by removal of the source. The source is never
//!   removed before the destination is verified.
//! * Every operation is journaled in SQLite *before* it starts, so an
//!   interrupted run can be reconciled on next launch ([`recover`]).
//! * Undo only deletes files whose content still matches what was written.

pub mod execute;
pub mod fsops;
pub mod plan;
pub mod undo;

pub use execute::{execute, recover, ExecProgress, ExecReport};
pub use plan::{build_plan, validate_output, Plan, PlanOptions, PlannedAction, PlannedOp, PlanSummary};
pub use undo::{undo_import, UndoReport};

#[cfg(test)]
mod tests;
