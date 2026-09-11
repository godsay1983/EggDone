import { tick } from "svelte";

export interface ScrollContext {
  positions: Map<string, number>;
  key: string;
  ready?: boolean;
}

// Each panel owns its map; no storage writes or cross-window state sharing.
export function preserveScroll(node: HTMLElement, initial: ScrollContext) {
  let context = initial;
  let generation = 0;
  let restored = false;
  let alive = true;
  function remember() {
    if (restored && context.ready !== false && Number.isFinite(node.scrollTop)) {
      context.positions.set(context.key, Math.max(0, node.scrollTop));
    }
  }
  async function restore() {
    const current = ++generation;
    restored = false;
    if (context.ready === false) return;
    await tick();
    if (!alive || current !== generation) return;
    node.scrollTop = context.positions.get(context.key) ?? 0;
    restored = true;
  }
  node.addEventListener("scroll", remember, { passive: true });
  void restore();
  return {
    update(next: ScrollContext) {
      const changed = next.key !== context.key || next.positions !== context.positions || next.ready !== context.ready;
      context = next;
      if (changed) void restore();
    },
    destroy() {
      alive = false;
      generation++;
      node.removeEventListener("scroll", remember);
    },
  };
}
