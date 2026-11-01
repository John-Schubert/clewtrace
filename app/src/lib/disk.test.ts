// @begin CT-EFF-008, CT-USE-009
// Tests for file watching in the UI: what happens to the open file when the disk changes,
// and the wait loop that hears of changes without polling.
import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => ({ watching: false })) }));

import { invoke } from '@tauri-apps/api/core';
import type { DiskNews } from './api';
import { decideDisk, DiskWatch } from './disk';
// @end

// @begin CT-EFF-008, CT-USE-009
function news(part: Partial<DiskNews>): DiskNews {
  return { watching: true, reloaded: [], conflicts: [], problems: [], summary: null, ...part };
}

describe('decideDisk', () => {
  it('does nothing when the open file did not change', () => {
    expect(decideDisk('a.sysml', false, news({ reloaded: ['b.sysml'] }))).toBe('none');
    expect(decideDisk(undefined, false, news({ reloaded: ['a.sysml'] }))).toBe('none');
  });

  it('reloads a clean editor', () => {
    expect(decideDisk('a.sysml', false, news({ reloaded: ['a.sysml'] }))).toBe('reload');
    // The core may still count opened text as its own; the editor knows it is clean.
    expect(decideDisk('a.sysml', false, news({ conflicts: ['a.sysml'] }))).toBe('reload');
  });

  it('asks when the editor has unsaved edits', () => {
    expect(decideDisk('a.sysml', true, news({ conflicts: ['a.sysml'] }))).toBe('ask');
    expect(decideDisk('a.sysml', true, news({ reloaded: ['a.sysml'] }))).toBe('ask');
  });
});

describe('DiskWatch', () => {
  it('waits again after each reply and stops when the project is not watched', async () => {
    const replies = [news({ reloaded: ['a.sysml'] }), news({ conflicts: ['b.sysml'] }), news({ watching: false })];
    const wait = vi.fn(async () => replies.shift()!);
    const heard: DiskNews[] = [];
    const watch = new DiskWatch(wait, (n) => void heard.push(n));
    await watch.start();
    expect(wait).toHaveBeenCalledTimes(3);
    expect(heard.map((n) => [...n.reloaded, ...n.conflicts])).toEqual([['a.sysml'], ['b.sysml']]);
    expect(watch.running).toBe(false);
  });

  it('runs one wait at a time however often it is started', async () => {
    let release: (n: DiskNews) => void = () => {};
    const wait = vi
      .fn<() => Promise<DiskNews>>()
      .mockImplementationOnce(() => new Promise<DiskNews>((r) => (release = r)))
      .mockResolvedValue(news({ watching: false }));
    const watch = new DiskWatch(wait, () => {});
    const first = watch.start();
    void watch.start();
    void watch.start();
    expect(wait).toHaveBeenCalledTimes(1);
    release(news({ watching: false }));
    await first;
    expect(watch.running).toBe(false);
  });

  it('waits once more when started while a wait for the last project was ending', async () => {
    // Opening another project ends the old wait with `watching: false`. When the new
    // project is ready before that reply arrives, the loop must not stop.
    let release: (n: DiskNews) => void = () => {};
    const wait = vi
      .fn<() => Promise<DiskNews>>()
      .mockImplementationOnce(() => new Promise<DiskNews>((r) => (release = r)))
      .mockResolvedValueOnce(news({ reloaded: ['a.sysml'] }))
      .mockResolvedValue(news({ watching: false }));
    const heard = vi.fn();
    const watch = new DiskWatch(wait, heard);
    const loop = watch.start();
    void watch.start();
    release(news({ watching: false }));
    await loop;
    expect(heard).toHaveBeenCalledOnce();
    expect(wait).toHaveBeenCalledTimes(3);
  });

  it('passes on the last reply when watching stops, such as trouble at the start', async () => {
    const heard: DiskNews[] = [];
    const trouble = { what: 'out of watches', where: '', help: 'raise the limit' };
    const watch = new DiskWatch(async () => news({ watching: false, problems: [trouble] }), (n) => void heard.push(n));
    await watch.start();
    expect(heard).toHaveLength(1);
  });

  it('stops after a failed wait instead of retrying in a loop', async () => {
    const failed = vi.fn();
    const watch = new DiskWatch(async () => Promise.reject(new Error('gone')), () => {}, failed);
    await watch.start();
    expect(failed).toHaveBeenCalledOnce();
    expect(watch.running).toBe(false);
  });

  it('asks the Rust side with the wait_for_disk_change command', async () => {
    const { waitForDiskChange } = await import('./api');
    await waitForDiskChange();
    expect(invoke).toHaveBeenCalledWith('wait_for_disk_change');
  });
});
// @end
