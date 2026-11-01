//! Clewtrace core library. Modules are added by later tasks in docs/PLAN.md.

// @begin CT-DEV-002, CT-DEV-004
pub mod check;
// @end
// @begin CT-DEV-005
pub mod commit;
// @end
// @begin CT-DEV-002, CT-DEV-004
pub mod config;
// @end
// @begin CT-MOD-001, CT-EFF-007, CT-SEC-003
pub mod index;
// @end
// @begin CT-USE-008, CT-SEC-003
pub mod io;
// @end
// @begin CT-MOD-002, CT-USE-007
pub mod model;
// @end
// @begin CT-DEV-014
pub mod json;
pub mod notices;
// @end
// @begin CT-MOD-002, CT-USE-007
pub mod parse;
// @end
// @begin CT-TRC-008, CT-USE-007
pub mod project;
// @end
// @begin CT-TRC-004, CT-TRC-005
pub mod report;
// @end
// @begin CT-TRC-009
pub mod reproduce;
// @end
// @begin CT-DEV-001, CT-TRC-001
pub mod reqs;
// @end
// @begin CT-DEV-002, CT-TRC-006
pub mod scan;
// @end
// @begin CT-TRC-002, CT-TRC-006
pub mod trace;
// @end
