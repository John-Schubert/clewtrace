// @begin CT-USE-008
// Tests for autosave: when it saves, that saves never overlap, and saving before the
// window closes.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock('@tauri-apps/api/event', async (original) => ({
  ...((await original()) as object),
  listen: vi.fn(async () => () => {}),
}));

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import conf from '../../src-tauri/tauri.conf.json';
import { AUTOSAVE_MS, Autosaver, MAIN_WINDOW, saveOnClose, windowCloseRequests, type CloseRequests } from './autosave';
// @end

// @begin CT-USE-008
/** A fake editor buffer: `dirty` until a save of the current version ends. */
function buffer() {
  const state = { version: 0, saved: 0, saves: [] as number[] };
  return {
    state,
    edit: () => void (state.version += 1),
    dirty: () => state.version !== state.saved,
  };
}

/** A save that ends when the test calls `finish`. */
function heldSave(b: ReturnType<typeof buffer>) {
  const ends: ((ok: boolean) => void)[] = [];
  const save = vi.fn(
    () =>
      new Promise<boolean>((resolve) => {
        const version = b.state.version;
        b.state.saves.push(version);
        ends.push((ok) => {
          if (ok) b.state.saved = version;
          resolve(ok);
        });
      }),
  );
  return { save, finish: (ok = true) => ends.shift()?.(ok) };
}
const tick = async () => {
  for (let i = 0; i < 5; i++) await Promise.resolve();
};
// @end

// @verifies CT-USE-008
describe('Autosaver', () => {
  beforeEach(() => void vi.useFakeTimers());
  afterEach(() => void vi.useRealTimers());

  // @verifies CT-USE-008
  it('saves once after a short pause in typing, not on each keystroke', async () => {
    const b = buffer();
    const save = vi.fn(async () => {
      b.state.saved = b.state.version;
      return true;
    });
    const auto = new Autosaver(save, b.dirty);
    for (let i = 0; i < 5; i++) {
      b.edit();
      auto.changed();
      vi.advanceTimersByTime(AUTOSAVE_MS - 1);
    }
    expect(save).not.toHaveBeenCalled();
    expect(auto.pending).toBe(true);
    vi.advanceTimersByTime(1);
    await tick();
    expect(save).toHaveBeenCalledTimes(1);
    expect(b.dirty()).toBe(false);
    expect(auto.pending).toBe(false);
    expect(vi.getTimerCount()).toBe(0);
  });

  // @verifies CT-USE-008
  it('saves now when flushed, and does nothing when there is nothing to save', async () => {
    const b = buffer();
    const save = vi.fn(async () => {
      b.state.saved = b.state.version;
      return true;
    });
    const auto = new Autosaver(save, b.dirty);
    expect(await auto.flush()).toBe(true);
    expect(save).not.toHaveBeenCalled();
    b.edit();
    auto.changed();
    expect(await auto.flush()).toBe(true);
    expect(save).toHaveBeenCalledTimes(1);
    expect(auto.pending).toBe(false);
    vi.advanceTimersByTime(AUTOSAVE_MS * 2);
    await tick();
    expect(save).toHaveBeenCalledTimes(1);
  });

  // @verifies CT-USE-008
  it('never runs two saves at once, and saves again for edits made during a save', async () => {
    const b = buffer();
    const { save, finish } = heldSave(b);
    const auto = new Autosaver(save, b.dirty);
    b.edit();
    const first = auto.flush();
    await tick();
    b.edit();
    const second = auto.flush();
    await tick();
    expect(save).toHaveBeenCalledTimes(1);
    finish();
    await tick();
    expect(save).toHaveBeenCalledTimes(2);
    finish();
    expect(await first).toBe(true);
    expect(await second).toBe(true);
    expect(b.state.saves).toEqual([1, 2]);
    expect(b.dirty()).toBe(false);
  });

  // @verifies CT-USE-008
  it('reports a failed save and keeps the edits unsaved', async () => {
    const b = buffer();
    const auto = new Autosaver(async () => false, b.dirty);
    b.edit();
    expect(await auto.flush()).toBe(false);
    expect(b.dirty()).toBe(true);
    const throws = new Autosaver(async () => {
      throw new Error('disk full');
    }, b.dirty);
    expect(await throws.flush()).toBe(false);
  });

  // @verifies CT-USE-008
  it('cancels a waiting save', async () => {
    const b = buffer();
    const save = vi.fn(async () => true);
    const auto = new Autosaver(save, b.dirty);
    b.edit();
    auto.changed();
    auto.cancel();
    vi.advanceTimersByTime(AUTOSAVE_MS * 2);
    await tick();
    expect(save).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  });
});

// @verifies CT-USE-008, CT-USE-007
describe('saveOnClose', () => {
  /** A fake window that sends one close request when `close` is called. */
  function fakeWindow() {
    let handler: Parameters<CloseRequests['onCloseRequested']>[0] | undefined;
    const unlisten = vi.fn();
    const win: CloseRequests = {
      onCloseRequested: async (h) => {
        handler = h;
        return unlisten;
      },
    };
    const close = async () => {
      let prevented = false;
      await handler!({ preventDefault: () => void (prevented = true) });
      return prevented ? 'kept open' : 'closed';
    };
    return { win, close, unlisten };
  }

  // @verifies CT-USE-008
  it('saves the edits before the window closes', async () => {
    const { win, close } = fakeWindow();
    const flush = vi.fn(async () => true);
    const failed = vi.fn();
    await saveOnClose(win, flush, failed);
    expect(await close()).toBe('closed');
    expect(flush).toHaveBeenCalledTimes(1);
    expect(failed).not.toHaveBeenCalled();
  });

  // @verifies CT-USE-008, CT-USE-007
  it('keeps the window open once when the save fails, then lets it close', async () => {
    const { win, close } = fakeWindow();
    const failed = vi.fn();
    await saveOnClose(win, async () => false, failed);
    expect(await close()).toBe('kept open');
    expect(failed).toHaveBeenCalledTimes(1);
    expect(await close()).toBe('closed');
    expect(failed).toHaveBeenCalledTimes(1);
  });

  // @verifies CT-USE-008
  it('returns the function that stops listening', async () => {
    const { win, unlisten } = fakeWindow();
    const stop = await saveOnClose(win, async () => true, () => {});
    stop();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});

// @verifies CT-USE-008
describe('windowCloseRequests', () => {
  // @verifies CT-USE-008
  it('names the window that tauri.conf.json opens', () => {
    expect(conf.app.windows.map((w) => w.label)).toEqual([MAIN_WINDOW]);
  });

  // @verifies CT-USE-008
  it('listens on its own window, so Tauri waits for the handler before closing', async () => {
    const handler = vi.fn(async () => {});
    await windowCloseRequests('main').onCloseRequested(handler);
    expect(listen).toHaveBeenCalledWith('tauri://close-requested', expect.any(Function), { target: { kind: 'Window', label: 'main' } });
    const heard = vi.mocked(listen).mock.calls[0][1] as () => Promise<void>;
    await heard();
    expect(handler).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith('plugin:window|destroy', { label: 'main' });
  });

  // @verifies CT-USE-008
  it('keeps the window open when the handler prevents the close', async () => {
    vi.mocked(listen).mockClear();
    vi.mocked(invoke).mockClear();
    await windowCloseRequests('main').onCloseRequested(async (event) => event.preventDefault());
    const heard = vi.mocked(listen).mock.calls[0][1] as () => Promise<void>;
    await heard();
    expect(invoke).not.toHaveBeenCalled();
  });
});
