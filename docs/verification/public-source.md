<!-- @verifies CT-DEV-009, CT-DEV-013 -->
# Public source and contribution policy check

Verification method: Inspection. Repeat before each release and record the result below.

## CT-DEV-009 Public source

- [x] `LICENSE` holds the Business Source License 1.1 text.
- [x] `LICENSE` states the change date (four years after release) and Apache-2.0 as the change license.
- [x] `LICENSING.md` summarizes the terms in plain words.
- [x] The repository is public on GitHub with issues enabled.
- [x] `rust-toolchain.toml` and the Cargo workspace let anyone build and test the source.

## CT-DEV-013 No outside code

- [x] `CONTRIBUTING.md` welcomes bug reports and suggestions through public issues.
- [x] `CONTRIBUTING.md` states that outside code contributions are not accepted.

## Record

| Date | Commit | Result | Checked by |
| --- | --- | --- | --- |
| 2026-10-21 | task 0.1 | Pass | Claude Code, issues setting confirmed by John |
