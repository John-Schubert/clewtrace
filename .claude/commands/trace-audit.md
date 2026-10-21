---
description: Run the trace gate and fix findings honestly
---

Run `cargo run -p clew-cli -- check --self` and summarize the findings by type.

For each untraced line or bad tag:
- If the code clearly serves an existing requirement, add or correct the tag.
- If the code serves no requirement, propose deleting it or propose a new requirement to John. Do not tag it just to pass.
- Never cite an ID that is not in model/requirements.sysml.

For oversized scopes, split the code or the tags so each scope is meaningful.

Rerun the check until it passes, then report what changed in three bullets or fewer.
