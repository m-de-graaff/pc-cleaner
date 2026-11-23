import { describe, expect, it } from "vitest";
import { formatBytes, progressLabel } from "./format";

describe("progressLabel", () => {
  it("is generic before any location finishes", () => {
    expect(progressLabel(null)).toBe("Scanning…");
  });

  it("counts finished locations and names the last one", () => {
    expect(progressLabel({ done: 12, total: 42, title: "pnpm store" })).toBe("Scanning… 12 of 42 checked (pnpm store)");
  });
});

describe("formatBytes", () => {
  it("matches the Rust formatter", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1023)).toBe("1023 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(1024 ** 3)).toBe("1.0 GB");
  });
});
