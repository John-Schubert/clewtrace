// @begin CT-USE-002
// Tests for the palette actions of the editor bar and the trace panel tabs.
import { describe, expect, it, vi } from 'vitest';
import { ActionRegistry } from './actions';
import { registerEditorActions, registerTraceTabActions } from './commands';

describe('editor actions', () => {
  // @verifies CT-USE-002
  it('save, undo, and redo run on the open editor with their shortcuts shown', async () => {
    const editor = { save: vi.fn().mockResolvedValue(true), undoEdit: vi.fn(), redoEdit: vi.fn() };
    const registry = new ActionRegistry();
    registerEditorActions(registry, () => editor);
    const byId = Object.fromEntries(registry.list().map((a) => [a.id, a]));
    expect(Object.keys(byId).sort()).toEqual(['editor.redo', 'editor.save', 'editor.undo']);
    expect(byId['editor.save'].shortcut).toBe('Ctrl+S');
    await registry.run('editor.save');
    await registry.run('editor.undo');
    await registry.run('editor.redo');
    expect(editor.save).toHaveBeenCalledOnce();
    expect(editor.undoEdit).toHaveBeenCalledOnce();
    expect(editor.redoEdit).toHaveBeenCalledOnce();
  });

  // @verifies CT-USE-002
  it('do nothing when no editor is open', async () => {
    const registry = new ActionRegistry();
    registerEditorActions(registry, () => undefined);
    for (const id of ['editor.save', 'editor.undo', 'editor.redo']) {
      await expect(registry.run(id)).resolves.toBeUndefined();
    }
  });
});

describe('trace tab actions', () => {
  // @verifies CT-USE-002
  it('switch the trace panel to links or impact', async () => {
    const set = vi.fn();
    const registry = new ActionRegistry();
    registerTraceTabActions(registry, set);
    await registry.run('trace.links');
    await registry.run('trace.impact');
    expect(set.mock.calls).toEqual([['links'], ['impact']]);
  });
});
// @end
