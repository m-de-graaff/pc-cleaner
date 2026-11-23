import type { Finding, Risk } from "./types";

export const RISK_ORDER: Risk[] = ["safe", "review", "caution"];

export const isSelectable = (f: Finding, elevated: boolean) => !f.requiresAdmin || elevated;

export function defaultSelection(findings: Finding[], elevated: boolean): Set<string> {
  return new Set(findings.filter((f) => f.risk === "safe" && isSelectable(f, elevated)).map((f) => f.id));
}

export function selectedTotal(findings: Finding[], selected: Set<string>): number {
  return findings.reduce((sum, f) => (selected.has(f.id) && !f.upperBound ? sum + f.sizeBytes : sum), 0);
}

/** Extra warnings for the confirmation dialog, based on what is selected. */
export function confirmNotes(findings: Finding[], selected: Set<string>): string[] {
  const kinds = new Set(findings.filter((f) => selected.has(f.id)).map((f) => f.action.kind));
  const notes: string[] = [];
  if (kinds.has("compactDisk")) {
    notes.push("Compacting shuts down WSL and Docker Desktop. Save your work in Linux and quit Docker Desktop first.");
  }
  if (kinds.has("dockerRemoveVolume") || kinds.has("deletePaths")) {
    notes.push("Docker volumes and Android emulators are deleted for good, with everything stored in them.");
  }
  return notes;
}

export function groupByRisk(findings: Finding[]): Record<Risk, Finding[]> {
  const groups: Record<Risk, Finding[]> = { safe: [], review: [], caution: [] };
  for (const f of findings) groups[f.risk].push(f);
  return groups;
}
