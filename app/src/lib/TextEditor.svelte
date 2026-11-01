<!-- @begin CT-MOD-004, CT-USE-003, CT-USE-008, CT-USE-007, CT-EFF-005 -->
<script lang="ts">
  // The text editor for one .sysml file. The Rust side serves the text and keywords and
  // saves it; the CodeMirror setup is in editor.ts. Saving is explicit: Save or Ctrl+S.
  import { onMount, untrack } from 'svelte';
  import { redo, undo } from '@codemirror/commands';
  import type { Text } from '@codemirror/state';
  import type { EditorView } from '@codemirror/view';
  import { editorKeywords, saveSource, sourceText } from './api';
  import { editorState, goToLine, openView } from './editor';

  let {
    path,
    line,
    ondirty,
  }: { path: string; line: number; ondirty: (dirty: boolean) => void } = $props();

  let host: HTMLElement;
  let editor: EditorView | undefined;
  let saved: Text | undefined;
  let keywords: Promise<string[]> | undefined;
  let loaded = $state(false);
  let dirty = $state(false);
  let saving = $state(false);
  let failure = $state('');
  let notice = $state('');
  let current = untrack(() => path);

  function message(e: unknown) {
    return e instanceof Error ? e.message : String(e);
  }

  function setDirty(next: boolean) {
    if (dirty !== next) {
      dirty = next;
      ondirty(next);
    }
  }

  // Saves the whole text. Resolves true when it was written.
  export async function save(): Promise<boolean> {
    if (!editor || saving) {
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
      return true;
    } catch (e) {
      failure = message(e);
      return false;
    } finally {
      saving = false;
    }
  }

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
    notice = '';
    loaded = false;
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
        },
        onSave: () => void save(),
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

  onMount(() => () => editor?.destroy());
</script>

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
  <div class="host" bind:this={host} hidden={!loaded && !!failure}></div>
</div>

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
