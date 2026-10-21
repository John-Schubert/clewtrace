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
// @begin CT-DEV-014
pub mod json;
pub mod notices;
// @end
// @begin CT-DEV-001, CT-TRC-001
pub mod reqs;
// @end
// @begin CT-DEV-002, CT-TRC-006
pub mod scan;
// @end

pub fn gate0_untraced_probe() {}
