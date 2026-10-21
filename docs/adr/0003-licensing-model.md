# ADR-0003: Licensing and business model

- Status: Accepted
- Date: 2026-10-20
- Serves: CT-DEV-009, CT-DEV-013, CT-PLT-005, CT-PLT-006, CT-PLT-007, CT-SEC-001, CT-SEC-006

## Context

John wants the source open to audit, commercial and government users to pay after a short
evaluation, personal non-commercial use to stay free, and nothing that conflicts with
offline, air-gapped use in CUI environments.

## Decision

- **License:** Business Source License 1.1 with an Additional Use Grant: personal,
  non-commercial use free with no time limit, organizational evaluation free for 90 days.
  Non-production use (reading, building, testing, auditing) is always free under the BSL.
  Each version converts to Apache-2.0 four years after first release.
- **Pricing:** annual subscription per user for commercial and government use. Government
  users pay like everyone else. Personal, non-commercial use is free.
- **Personal use (changed by John, 2026-10-21):** the earlier 12-month limit on personal,
  non-commercial use is removed from the Additional Use Grant. At first run the app asks
  whether use is personal or an organizational evaluation. It cannot verify the answer; the
  license terms carry the obligation (CT-PLT-006).
- **Contributions:** no outside code contributions, so no contributor agreement is needed.
- **Enforcement:** an Ed25519-signed license file verified offline through a vetted library.
  No activation server, no network calls.
- **Expiry behavior:** projects stay readable and exportable; only editing is disabled.

## Third-party licenses (decided by John, 2026-10-21)

Two tiers, because license duties apply to code we distribute and build tools are not
distributed.

- **Shipped code** (Rust crates linked into the app or CLI, npm production dependencies in
  the web bundle): MIT, Apache-2.0, Unicode-3.0, and Zlib, plus per-crate exceptions John
  has approved. Any new license needs John's approval first.
- **Build-only tools** (build scripts, proc macros, npm dev dependencies): any permissive
  license is fine without asking: MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib,
  Unicode-3.0, CC0-1.0, Apache-2.0 WITH LLVM-exception, and approved MPL-2.0 crates.
- **Never, in either tier:** GPL, AGPL, LGPL, and SSPL.
- **MPL-2.0** is allowed only per crate, never on the general allowlist. Approved so far:
  cssparser, cssparser-macros, dtoa-short, and selectors (build-only), and option-ext
  (shipped). For each shipped MPL crate, each release attaches its unmodified source and
  the CT-DEV-014 notices file lists its crate, version, and source link.
- `deny.toml` enforces this for Rust crates and `deps.toml` records each exception with its
  scope. npm production and dev dependencies are checked separately (task 0.9), and the SBOM
  marks each dependency as shipped or build-only.
- Confirm this policy in the attorney review planned below, before the first sale.

## Alternatives rejected

- **PolyForm Noncommercial:** lets government institutions use the software free.
- **PolyForm Small Business:** gives away most small defense subcontractors.
- **PolyForm Free Trial:** fixed at under 32 days for everyone.
- **Custom proprietary license:** needs an attorney to draft and loses the open-audit story.
- **Online activation:** violates CT-SEC-001 and fails on air-gapped networks.

## Consequences

- Anyone could build the public source without the license check. The license forbids
  production use of such builds; official signed builds are the supported path.
- The BSL text may not be modified except for its parameters, per its Covenants of Licensor.
- Have an attorney review the commercial subscription terms before the first sale.
