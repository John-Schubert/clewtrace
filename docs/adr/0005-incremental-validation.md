# ADR-0005: Incremental validation and stable element IDs

- Status: Proposed (for John's review in the 2.9a pull request)
- Date: 2026-11-01
- Serves: CT-MOD-003, CT-EFF-007, CT-EFF-004

## Context

Live validation in 2.9b sends the editor buffer to the core after a short pause in typing.
The core must report syntax and reference errors with file, line, and column within 1 s
of an edit in a file of up to 10,000 lines (CT-MOD-003). It must re-parse only the file
that changed (CT-EFF-007), and the open project must stay within 400 MB (CT-EFF-004).

A name in one file can resolve to an element in any other file, and a trace link can
cross files. So an edit can change errors and links anywhere in the project, not only in
the edited file. Element IDs were model indexes (ADR-0004), so every element after an
edit point got a new ID on a rebuild, and the UI would lose its selection and history.

## Decision

- **Cache the inputs, not the results.** `clew_core::live::Sources` holds what each
  file gave when read once: the syntax tree and rendered syntax errors of each model file,
  the tag scan of each code file, the checklists, the documents, and the SHA-256 of each.
  `Sources::project` builds the model, names, requirements, trace graph, and problem list
  from them. `load_project` is now `Sources::read` then `Sources::project`, so the CLI and
  the live path build a project the same way.
- **Re-parse one file, rebuild the rest.** `Workspace::edit(path, text)` parses the new
  text, replaces that file's cached tree (or adds a new `.sysml` file in path order), and
  calls `Sources::project` again. Name resolution and the trace graph are rebuilt in full
  from the cached trees. They are fast next to parsing and reading, and a full rebuild
  avoids tracking which names in other files could resolve differently.
- **The buffer is the truth for the edit.** Nothing is written to disk. The project's
  `inputs` hash the buffer text, so a report built from the workspace says what it read.
- **One project at a time.** `edit` drops the old project before it builds the new one,
  so the two are never held together (CT-EFF-004).
- **A broken setup rebuilds the last project.** If the edit makes `trace.toml` fail, such
  as a `top_level_definitions` entry that no longer resolves, `edit` returns the
  `SetupError`. It puts the file's previous syntax tree back in the cache and rebuilds the
  last good project from it, with no parse. The cache therefore always holds the inputs
  of the current project, so that rebuild cannot fail. The editor sends the file's text
  again on its next edit.
- **Stable IDs.** `Workspace::stable_id` gives each element a `u64` that survives a
  rebuild. Its key is its owner's stable ID, its kind, its name (or short name), and how
  many earlier siblings share that kind and name. A top-level element is keyed under its
  file path, so same-named packages in two files stay apart. The root is 0. IDs of
  elements that are gone are dropped, so the table does not grow while the user types.
  `Workspace::element` maps a stable ID back to the current model index.

## Alternatives rejected

- **Re-resolve only the names that could change.** Faster in theory, but a name's target
  depends on imports, inheritance, and every file's top-level members. Getting the
  invalidation right is hard to test, and the full rebuild already meets the budget.
- **Stable IDs from qualified names.** Unnamed elements such as `satisfy` and dependencies
  have no name, and same-named packages in two files share one.
- **Stable IDs from file and position.** Typing above an element moves it, so its ID
  would change on almost every keystroke.

## Consequences

- The workspace keeps every syntax tree in memory. In the perf job's 10,000-requirement
  project, the core's peak resident memory went from 138 MB to 189 MB on a 4 CPU
  machine. A first version that held the old and new project together during an edit
  peaked at 243 MB. An edit in a 10,000-line file took 334 to 390 ms over three runs.
  Both are checked by `crates/clew-core/tests/perf.rs`.
- **Estimated app total: about 384 MB, under the 400 MB budget with little margin.** The
  2.6 in-app probe measured 333 MB total PSS for the same project when the core peaked
  at 138 MB, so the app adds about 195 MB. 195 MB plus the core's 189 MB is 384 MB. This
  is an estimate. The 2.6 probe, run again after 2.9b moves the app to `Workspace`,
  gives the real figure.
- A setup error costs a second rebuild, since the last project was dropped. Such errors
  are rare while typing, and the rebuild is about as fast as the edit itself.
- An element gets a new stable ID when it is renamed, changes kind, moves to another owner
  or file, or when a same-named sibling is added before it. Unnamed elements, such as
  `satisfy`, are told apart by order only, so adding one before another shifts their IDs.
  The UI drops a selection whose ID is gone.
- Code files, checklists, and documents are read once at `open`. A change to them on disk
  is picked up by file watching in 2.10b.
