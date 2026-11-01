<script lang="ts">
  // @begin CT-MOD-004, CT-MOD-003, CT-USE-003, CT-USE-008, CT-USE-007, CT-EFF-005, CT-EFF-008, CT-USE-009
  // The text editor for one .sysml file. The Rust side serves the text and keywords and
  // saves it; the CodeMirror setup is in editor.ts. Edits are saved by themselves after a
  // short pause and when the editor loses focus (autosave.ts), or at once with Save or
  // Ctrl+S.
  // After a short pause in typing the unsaved text goes to the Rust side to be checked, and
  // the problems it finds in this file come back as markers in the text.
  import { onMount, untrack } from 'svelte';
  import { redo, undo } from '@codemirror/commands';
  import type { Text } from '@codemirror/state';
  import type { EditorView } from '@codemirror/view';
  import { editorKeywords, saveSource, sourceText, validateSource, type EditReply } from './api';
  import { editorState, goToLine, openView, showDiagnostics } from './editor';
  import { AUTOSAVE_MS, Autosaver } from './autosave';
  import { markers, QUIET_MS, QuietTimer } from './live';

  let {
    path,
    line,
    ondirty,
    onvalidated,
    quietMs = QUIET_MS,
    autosaveMs = AUTOSAVE_MS,
  }: {
    path: string;
    line: number;
    ondirty: (dirty: boolean) => void;
    /** The project was rebuilt from this file's unsaved text; the panes should refresh. */
    onvalidated: (reply: EditReply) => void;
    quietMs?: number;
    autosaveMs?: number;
  } = $props();

  let host: HTMLElement;
  let editor: EditorView | undefined;
  let saved: Text | undefined;
  let keywords: Promise<string[]> | undefined;
  let loaded = $state(false);
  let dirty = $state(false);
  let saving = $state(false);
  let failure = $state('');
  let notice = $state('');
  // While the user is asked whether to keep their edits or take the file as it changed on
  // disk, nothing is saved, so the answer is theirs and not the autosave's.
  let held = false;
  let current = untrack(() => path);
  // Live validation. `edits` counts changes to the text, so a reply for older text shows no
  // markers. One check is out at a time; typing during it asks for another after it ends.
  const quiet = new QuietTimer(untrack(() => quietMs));
  let edits = 0;
  let checking = false;
  let checkFailure = $state('');

  function message(e: unknown) {
    return e instanceof Error ? e.message : String(e);
  }

  function setDirty(next: boolean) {
    if (dirty !== next) {
      dirty = next;
      ondirty(next);
    }
  }

  // Saves the whole text. Resolves true when it was written. Only the autosaver calls it,
  // so one save runs at a time.
  async function write(): Promise<boolean> {
    if (!editor || saving || held) {
      return false;
    }
    const doc = editor.state.doc;
    saving = true;
    failure = '';
    try {
      await saveSource(current, doc.toString());
      saved = doc;
      notice = 'Saved';
      setDirty(!editor.state.doc.eq(doc));
      // The project must match what is on disk, even when the pause had not ended yet.
      quiet.cancel();
      void check();
      return true;
    } catch (e) {
      failure = message(e);
      return false;
    } finally {
      saving = false;
    }
  }

  // Sends the text as it is now. `opening` is the check of the saved text when the file is
  // opened, which shows its markers and tells the other panes nothing, since nothing changed.
  async function check(opening = false) {
    if (!editor) {
      return;
    }
    if (checking) {
      quiet.schedule(() => void check());
      return;
    }
    const file = current;
    const sent = edits;
    const doc = editor.state.doc;
    checking = true;
    try {
      const reply = await validateSource(file, doc.toString());
      checkFailure = '';
      if (editor && file === current && sent === edits) {
        showDiagnostics(editor, markers(editor.state.doc, reply.diagnostics));
      }
      if (!opening && file === current) {
        onvalidated(reply);
      }
    } catch (e) {
      checkFailure = message(e);
    } finally {
      checking = false;
    }
  }

  const autosave = new Autosaver(write, () => dirty, untrack(() => autosaveMs));

  // Saves now, after any save already running. Resolves true when nothing is left unsaved.
  export function save(): Promise<boolean> {
    return autosave.flush();
  }

  // The file changed on disk while it has unsaved edits: stop saving until the user says
  // which text to keep.
  export function hold() {
    held = true;
    autosave.cancel();
  }

  // The user answered. Keeping the edits saves them over the disk version after the pause;
  // otherwise the host reloads the disk text.
  export function release(keep: boolean) {
    held = false;
    if (keep && dirty) {
      autosave.changed();
    }
  }

  // Takes the file's text from disk again, after it changed there, and keeps the cursor on
  // its line. The undo history starts afresh, as on open.
  export function reloadFromDisk(): Promise<void> {
    const at = editor ? editor.state.doc.lineAt(editor.state.selection.main.head).number : 1;
    return load(current, at);
  }

  // @end

  // @begin CT-MOD-004, CT-MOD-003, CT-USE-003, CT-USE-008, CT-USE-007, CT-EFF-005, CT-EFF-008
  export function view(): EditorView | undefined {
    return editor;
  }

  export function goto(target: number) {
    if (editor) {
      goToLine(editor, target);
    }
  }

  async function load(file: string, target: number) {
    failure = '';
    checkFailure = '';
    notice = '';
    loaded = false;
    quiet.cancel();
    autosave.cancel();
    edits += 1;
    try {
      keywords ??= editorKeywords();
      const [{ text }, words] = await Promise.all([sourceText(file), keywords]);
      if (file !== current) {
        return;
      }
      const state = editorState(text, words, {
        onChange: (doc) => {
          notice = '';
          setDirty(!saved?.eq(doc));
          edits += 1;
          quiet.schedule(() => void check());
          autosave.changed();
        },
        onSave: () => void save(),
        onBlur: () => void save(),
      });
      saved = state.doc;
      if (editor) {
        editor.setState(state);
      } else {
        editor = openView(host, state);
      }
      setDirty(false);
      loaded = true;
      goToLine(editor, target);
      void check(true);
    } catch (e) {
      failure = message(e);
    }
  }

  // Opening another file replaces the text; the host has already asked about unsaved edits.
  $effect(() => {
    current = path;
    const target = untrack(() => line);
    void load(path, target);
  });

  onMount(() => () => {
    quiet.cancel();
    autosave.cancel();
    editor?.destroy();
  });
  // @end
</script>

<!-- @begin CT-MOD-004, CT-MOD-003, CT-USE-003, CT-USE-008, CT-USE-007, CT-EFF-005, CT-EFF-008 -->
<div class="editor" data-view="editor">
  <h2>{path}</h2>
  <div class="bar" role="toolbar" aria-label="Editor actions">
    <button type="button" disabled={!dirty || saving} onclick={() => void save()}>Save</button>
    <button type="button" disabled={!loaded} onclick={() => editor && undo(editor)}>Undo</button>
    <button type="button" disabled={!loaded} onclick={() => editor && redo(editor)}>Redo</button>
    <span class="status" role="status">{dirty ? 'Unsaved changes' : notice}</span>
  </div>
  {#if failure}
    <p class="problem" role="alert">{failure}</p>
  {/if}
  {#if checkFailure}
    <p class="problem" role="alert">Could not check the text: {checkFailure}</p>
  {/if}
  <div class="host" bind:this={host} hidden={!loaded && !!failure}></div>
</div>
<!-- @end -->

<!-- @begin CT-MOD-004, CT-USE-004 -->
<style>
  .editor {
    display: flex;
    flex-direction: column;
    height: calc(100% - 2rem);
    min-height: 8rem;
  }
  h2 {
    margin: 0 0 0.5rem;
    font-size: 1rem;
    overflow-wrap: anywhere;
  }
  .bar {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    margin-bottom: 0.5rem;
  }
  .status {
    color: var(--text-muted);
  }
  .host {
    flex: 1;
    min-height: 5rem;
    border: 1px solid var(--border);
  }
  .problem {
    color: var(--danger);
    margin: 0 0 0.5rem;
  }
</style>
<!-- @end -->
