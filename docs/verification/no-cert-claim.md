<!-- @verifies CT-CMP-008 -->
# No certification claim check

Verification method: Inspection. Repeat before each release and whenever user-facing text
changes: the app UI, installer text, README, LICENSING, INSTALL, and release notes. Record
the result below.

## What to look for

Any wording that says or implies that using Clewtrace makes an organization CMMC compliant,
CMMC certified, NIST SP 800-171 compliant, DFARS compliant, or accredited. Saying that
Clewtrace helps trace controls is fine. A plain disclaimer, such as the one in `LICENSING.md`,
is fine.

A quick search for candidates:

```sh
grep -rniE "complian|certif|cmmc|800-171|dfars|accredit" --include=*.md --include=*.svelte \
  --include=*.ts --include=*.html --include=*.json . | grep -v node_modules
```

## Checklist

- [x] `README.md` makes no compliance or certification claim.
- [x] `LICENSING.md` says that using Clewtrace does not make an organization CMMC compliant
      or certified.
- [x] `docs/INSTALL.md` makes no compliance or certification claim.
- [x] The app UI (`app/src`) and window title make no compliance or certification claim.
- [x] The installer metadata (`app/src-tauri/tauri.conf.json`,
      `app/src-tauri/tauri.release.conf.json`) makes no compliance or certification claim.
- [x] The GitHub repository description makes no compliance or certification claim.

## Record

| Date | Commit | Result | Checked by |
| --- | --- | --- | --- |
| 2026-10-22 | task 1.9 | Pass. The search finds only the `LICENSING.md` disclaimer outside the plan, the requirements, and `CLAUDE.md` | Claude Code |
