# ADR-0003: Licensing and business model

- Status: Accepted
- Date: 2026-10-20
- Serves: CT-DEV-009, CT-DEV-013, CT-PLT-005, CT-PLT-006, CT-PLT-007, CT-SEC-001, CT-SEC-006

## Context

John wants the source open to audit, every user to pay eventually, short free periods, and
nothing that conflicts with offline, air-gapped use in CUI environments.

## Decision

- **License:** Business Source License 1.1 with an Additional Use Grant: personal,
  non-commercial use free for 12 months, organizational evaluation free for 90 days.
  Non-production use (reading, building, testing, auditing) is always free under the BSL.
  Each version converts to Apache-2.0 four years after first release.
- **Pricing:** annual subscription per user. Government users pay like everyone else.
- **Contributions:** no outside code contributions, so no contributor agreement is needed.
- **Enforcement:** an Ed25519-signed license file verified offline through a vetted library.
  No activation server, no network calls.
- **Expiry behavior:** projects stay readable and exportable; only editing is disabled.

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
