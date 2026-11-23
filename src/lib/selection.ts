import type { Finding, Risk } from "./types";

export const RISK_ORDER: Risk[] = ["safe", "review", "caution"];

export const isSelectable = (f: Finding, elevated: boolean) => !f.requiresAdmin || elevated;

export function defaultSelection(findings: Finding[], elevated: boolean): Set<string> {
  return new Set(findings.filter((f) => f.risk === "safe" && isSelectable(f, elevated)).map((f) => f.id));
}

export function selectedTotal(findings: Finding[], selected: Set<string>): number {
  return findings.reduce((sum, f) => (selected.has(f.id) ? sum + f.sizeBytes : sum), 0);
}

export function groupByRisk(findings: Finding[]): Record<Risk, Finding[]> {
  const groups: Record<Risk, Finding[]> = { safe: [], review: [], caution: [] };
  for (const f of findings) groups[f.risk].push(f);
  return groups;
}
