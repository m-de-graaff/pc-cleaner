import { describe, expect, it } from "vitest";
import { confirmNotes, defaultSelection, groupByRisk, isSelectable, selectedTotal } from "./selection";
import type { Finding, Risk } from "./types";

const f = (id: string, risk: Risk, sizeBytes: number, requiresAdmin = false): Finding => ({
  id, risk, sizeBytes, requiresAdmin,
  category: "junk", title: id, itemCount: 1, lastModified: null, score: 50, reason: "", reversible: false, upperBound: false,
  action: { kind: "emptyRecycleBin" },
});

const items = [f("a", "safe", 100), f("b", "review", 200), f("c", "safe", 300, true), f("d", "caution", 1)];

describe("selection", () => {
  it("pre-selects only safe items the user can run", () => {
    expect([...defaultSelection(items, false)]).toEqual(["a"]);
    expect([...defaultSelection(items, true)].sort()).toEqual(["a", "c"]);
  });

  it("admin items are not selectable without elevation", () => {
    expect(isSelectable(items[2], false)).toBe(false);
    expect(isSelectable(items[2], true)).toBe(true);
  });

  it("totals selected sizes", () => {
    expect(selectedTotal(items, new Set(["a", "b"]))).toBe(300);
    expect(selectedTotal(items, new Set())).toBe(0);
  });

  it("leaves upper-bound sizes out of the total", () => {
    const disk: Finding = { ...f("w", "review", 5000), upperBound: true, action: { kind: "compactDisk", path: "C:\\x.vhdx" } };
    expect(selectedTotal([...items, disk], new Set(["a", "w"]))).toBe(100);
  });

  it("warns about WSL shutdown and permanent data loss only when relevant", () => {
    const disk: Finding = { ...f("w", "review", 1), action: { kind: "compactDisk", path: "C:\\x.vhdx" } };
    const vol: Finding = { ...f("v", "caution", 1), action: { kind: "dockerRemoveVolume", name: "pg" } };
    const all = [...items, disk, vol];
    expect(confirmNotes(all, new Set(["a"]))).toEqual([]);
    expect(confirmNotes(all, new Set(["w"]))[0]).toMatch(/shuts down WSL/);
    expect(confirmNotes(all, new Set(["v", "w"]))).toHaveLength(2);
  });

  it("groups by risk in a fixed order", () => {
    const g = groupByRisk(items);
    expect(Object.keys(g)).toEqual(["safe", "review", "caution"]);
    expect(g.safe.map((x) => x.id)).toEqual(["a", "c"]);
  });
});
