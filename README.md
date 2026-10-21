# Clewtrace

Local-first SysML v2 and requirements traceability for Windows, macOS, and Linux. Small
enough for a 4 GB laptop, fully offline, and built so that every line of its own code traces
to a requirement.

A clew is the ball of thread Ariadne gave Theseus to trace his way out of the labyrinth.
Clewtrace follows that thread from each requirement to the code, tests, and evidence behind it.

Status: kickoff. No application code yet. Source-available under the Business Source
License 1.1; see `LICENSING.md`.

## What is in this package

| Path | Purpose |
| --- | --- |
| `CLAUDE.md` | Standing instructions Claude Code reads every session |
| `model/requirements.sysml` | The 75 product requirements in SysML v2 text; the source of truth for every trace tag |
| `docs/TRACEABILITY.md` | How code, tests, and commits trace to requirements, and what CI enforces |
| `docs/PLAN.md` | Session-sized tasks for phases 0 and 1, with exit gates |
| `docs/adr/` | Architecture decisions: stack, trace annotations, licensing |
| `LICENSE`, `LICENSING.md` | Business Source License 1.1 with Clewtrace's terms, and a plain-words summary |
| `CONTRIBUTING.md` | Bug reports welcome; no outside code contributions |
| `trace.toml` | Settings for `clew check --self` (current phase, exclusions, waivers) |
| `deps.toml` | Every dependency with license, reason, and requirement served |
| `.gitmessage` | Commit template with the required `Refs:` trailer |
| `.claude/commands/` | `/implement`, `/trace-audit`, and `/next-task` shortcuts for Claude Code |

## One-time setup

1. **Create the repository.** On github.com, create a new **public** repository named
   `clewtrace` with no README, license, or .gitignore (this package has them).
2. **Push this package from your Pi 5.** Download the zip there, then:

```
cd ~/Downloads && unzip clewtrace-kickoff.zip && cd clewtrace-kickoff
git init -b main
git config commit.template .gitmessage
git add .
git commit -m "Add kickoff package" -m "Refs: CT-DEV-001, CT-DEV-009, CT-DEV-010"
git remote add origin https://github.com/John-Schubert/clewtrace.git
git push -u origin main
```

3. **Connect Claude Code on the web.** Open claude.ai/code, connect GitHub, and install the
   Claude GitHub App on the `clewtrace` repository when asked.
4. **Configure the cloud environment** (environment settings at claude.ai/code):
   - Network access: **Trusted** (the default; it reaches crates.io, npm, and Ubuntu packages).
   - Environment variables, so Rust builds are not cut off at 2 minutes:
     `BASH_DEFAULT_TIMEOUT_MS=600000` and `BASH_MAX_TIMEOUT_MS=600000`
   - Setup script, needed from task 0.7 on (Tauri's Linux build packages):

```
#!/bin/bash
apt-get update
apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

5. Make sure no `ANTHROPIC_API_KEY` is set anywhere you run Claude Code, so it uses your Pro
   plan instead of billing the API.

Your laptop (Windows 10, 4 GB RAM) is the reference test machine: download installers from
GitHub Actions and try them there. Nothing needs to be compiled on the laptop or the Pi.

## First Claude Code session

Start a cloud session on the `clewtrace` repository and send:

```
Read CLAUDE.md, docs/TRACEABILITY.md, and docs/PLAN.md. Confirm in five bullets or fewer
how you will trace code to requirements, then run /implement 0.1.
```

After that, each task gets its own new session: `/next-task`, then `/implement <task>`.
If a slash command is not available, send "Follow .claude/commands/implement.md for task 0.2"
instead. Until task 0.4 builds the trace checker, tag discipline relies on review; task 0.4
then checks everything written before it.

## Core tenets

Traceability, efficiency, usability, in that order when they conflict.
