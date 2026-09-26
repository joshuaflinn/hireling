//! The Pathbuilder import pipeline (epic E5).
//!
//! Paste/upload a Pathbuilder 2e export; the character derives from it, is
//! owned by the importing account, and re-imports preserve all live table
//! state by exact anchors. Shape per `specs/005-pathbuilder-import/`: a pure
//! parse→validate→transform→anchor core with thin store/HTTP shells —
//!
//! - [`caps`]: server-side size/depth caps (FR-3)
//! - (later tasks: model, transform, anchor, store, handlers)
//!
//! The input contract is the frozen fixture `tests/data/pb_export_reference.json`
//! (captured from `docs/reference/lorum_ipsum_dashboard.html`, `#pbExport`);
//! `contracts/pb-export.md` cites it, never prose memory.

#[cfg(test)]
#[path = "tests/fixtures.rs"]
pub(crate) mod fixtures;
