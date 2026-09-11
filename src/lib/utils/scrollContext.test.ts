import { describe, it, expect } from 'vitest';
import { preserveScroll } from './scrollContext';

function element() {
  const listeners = new Map<string, EventListener>();
  const fake = {
    scrollTop: 0,
    addEventListener(name: string, fn: EventListener) { listeners.set(name, fn); },
    removeEventListener(name: string) { listeners.delete(name); },
  };
  return { node: fake as unknown as HTMLElement, scroll(y: number) {
    fake.scrollTop = y; listeners.get('scroll')?.({} as Event);
  }, listeners };
}
const settle = async () => { await Promise.resolve(); await Promise.resolve(); await Promise.resolve(); };
describe('session scroll context', () => {
  it('restores independent task/note positions after remount', async () => {
    const positions = new Map([['tasks', 350], ['notes', 720]]);
    const e = element();
    const action = preserveScroll(e.node, { positions, key: 'tasks' });
    await settle(); expect(e.node.scrollTop).toBe(350);
    e.scroll(500); action.destroy();
    expect(e.listeners.size).toBe(0);
    const n = element(); preserveScroll(n.node, { positions, key: 'notes' });
    const t = element(); preserveScroll(t.node, { positions, key: 'tasks' });
    await settle(); expect(n.node.scrollTop).toBe(720); expect(t.node.scrollTop).toBe(500);
  });
  it('loading layout and detached callbacks do not erase saved positions', async () => {
    const positions = new Map([['notes', 700]]), e = element();
    const action = preserveScroll(e.node, { positions, key: 'notes', ready: false });
    e.scroll(0); await settle(); expect(positions.get('notes')).toBe(700);
    action.update({ positions, key: 'notes', ready: true });
    await settle(); expect(e.node.scrollTop).toBe(700);
    action.destroy(); e.scroll(0); expect(positions.get('notes')).toBe(700);
  });
  it('stale scheduled restore cannot update a destroyed or newer view', async () => {
    const positions = new Map([['tasks', 100], ['notes', 600]]), e = element();
    const action = preserveScroll(e.node, { positions, key: 'tasks' });
    action.update({ positions, key: 'notes' });
    await settle(); expect(e.node.scrollTop).toBe(600);
    action.update({ positions, key: 'tasks' }); action.destroy();
    await settle(); expect(e.node.scrollTop).toBe(600);
  });
  it('unrelated render updates do not jump; invalid offsets are ignored', async () => {
    const positions = new Map<string, number>(), e = element();
    const context = { positions, key: 'notes' };
    const action = preserveScroll(e.node, context); await settle();
    e.scroll(320); action.update({ ...context }); await settle();
    expect(e.node.scrollTop).toBe(320);
    e.scroll(NaN); expect(positions.get('notes')).toBe(320);
    e.scroll(-1); expect(positions.get('notes')).toBe(0);
  });
});
