<!-- @verifies CT-DEV-012, CT-DEV-014, CT-PLT-001, CT-EFF-001 -->
# Release check

Verification method: Inspection for CT-DEV-012 and CT-DEV-014, backed by the release
workflow (`.github/workflows/release.yml`), which is the test for CT-PLT-001 and CT-EFF-001.
Go through this list for every release, before publishing the draft, and record the result.

## Before the first release

- [ ] `.github/release-signers` holds John's dedicated, passphrase-protected SSH release key
      (git allowed-signers format). The `verify tag` job fails until a signer file exists.
      The reviewer of task 0.10 adds it; no one else edits it without John.

## Automated in the release workflow

- `verify tag`: the tag is annotated and signed by a key in `.github/release-signers` or
  `.github/release-keys.asc`, it points to a commit on `main`, and it matches the version in
  `Cargo.toml` and `app/package.json`. Nothing else runs if this fails.
- `build`: MSI on Windows, a universal DMG on macOS, deb and rpm on Ubuntu 22.04. The job
  fails if any installer is over 20,000,000 bytes or a bundle type is missing.
- `notices`: `clew notices --out target/notices` writes `THIRD-PARTY-NOTICES.txt`,
  `sbom.cdx.json`, and `mpl-sources/`, and fails on any license policy error.
- `finish`: attaches the notices file, the SBOM, each shipped MPL-2.0 `.crate` archive, and
  `SHA256SUMS.txt` covering every attachment.

## CT-DEV-012 Releases from CI

- [ ] The draft release was created by the `release` workflow run for this tag, and every
      job in that run passed.
- [ ] `git verify-tag <tag>` succeeds locally with John's key.
- [ ] No asset on the release was uploaded by hand.

## CT-DEV-014 License notices

- [ ] `THIRD-PARTY-NOTICES.txt` is attached and is inside each installer: next to the
      program on Windows, in `Clewtrace.app/Contents/Resources` on macOS, and in
      `/usr/lib/Clewtrace` for the deb and rpm.
- [ ] The notices file lists every shipped crate and npm package, and no build-only one.
      Spot-check five entries against `cargo tree -e normal,no-proc-macro` and the
      production packages in `app/package-lock.json`.
- [ ] The packages the run warned about (no license file in the package) were checked by
      hand. Their license and repository link are in the notices file.
- [ ] Each shipped MPL-2.0 crate (`scope = "shipped"` in `deps.toml`, today `option-ext`)
      has its `.crate` archive attached, and the notices file names its version and source
      link. The attached archive matches the one on crates.io (same SHA-256).
- [ ] `sbom.cdx.json` marks each package `shipped` (scope `required`) or `build-only`
      (scope `excluded`), and the shipped ones match the notices file.

## CT-PLT-001 and CT-EFF-001 Installers

- [ ] MSI, DMG, deb, and rpm are attached, each 20 MB or smaller, and each matches its line
      in `SHA256SUMS.txt` (`sha256sum -c SHA256SUMS.txt`).
- [ ] No AppImage or other bundle that includes GTK or WebKitGTK is attached (ADR-0001).
- [ ] The installer opens the app on Windows 10, macOS 13, and Ubuntu (Gate 0).
- [ ] `docs/INSTALL.md` still matches the installers, including the offline WebView2 step.

## Record

| Date | Tag | Result | Checked by |
| --- | --- | --- | --- |
| 2026-10-21 | v0.1.0 | Draft built from the signed tag. Checksums match. The MSI installed and ran on the Windows 10 reference laptop. Published 2026-10-21. | John (laptop), Claude (checksums, notices) |
