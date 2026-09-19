import { describe, expect, it } from "vitest";
import { RemotePollState } from "./remotePollState";

describe("planning remote polling", () => {
  it("consumes only sync receipts, not probe observations", () => {
    const state = new RemotePollState();
    expect(state.plansChanged(undefined)).toBe(false);
    expect(state.plansChanged("missing")).toBe(true);
    expect(state.plansChanged("missing")).toBe(true);
    state.acknowledgePlans(state.beginSync(), "missing");
    expect(state.plansChanged("missing")).toBe(false);
    expect(state.plansChanged('etag:"new"')).toBe(true);
    expect(state.plansChanged('etag:"new"')).toBe(true);
  });

  it("rejects old-target receipts and probes after configuration changes", () => {
    const state = new RemotePollState();
    const generation = state.beginSync();
    const ticket = state.capture();
    state.reset();
    state.acknowledgePlans(generation, 'etag:"old"');
    expect(state.isCurrent(ticket)).toBe(false);
    expect(state.plansChanged('etag:"old"')).toBe(true);
    state.acknowledgePlans(state.beginSync(), 'etag:"new"');
    expect(state.plansChanged('etag:"new"')).toBe(false);
    expect(state.plansChanged("missing")).toBe(true);
  });
});
