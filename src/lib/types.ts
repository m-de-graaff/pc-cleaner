export type Risk = "safe" | "review" | "caution";
export type Category = "junk";

export interface NameFilter { prefix: string | null; suffix: string | null }

export type Action =
  | { kind: "deleteContents"; root: string; filter: NameFilter; minAgeHours: number }
  | { kind: "emptyRecycleBin" };

export interface Finding {
  id: string;
  category: Category;
  title: string;
  sizeBytes: number;
  itemCount: number;
  lastModified: number | null;
  risk: Risk;
  score: number;
  reason: string;
  requiresAdmin: boolean;
  reversible: boolean;
  action: Action;
}

export interface ItemResult {
  findingId: string;
  freedBytes: number;
  deletedCount: number;
  failedCount: number;
  errors: string[];
  skipped: string | null;
}

export interface ExecReport { items: ItemResult[]; totalFreed: number; dryRun: boolean }

export interface JournalEntry {
  timestamp: number;
  findingId: string;
  title: string;
  category: Category;
  actionKind: string;
  freedBytes: number;
  deletedCount: number;
  failedCount: number;
  dryRun: boolean;
  reversible: boolean;
}

export interface ScanProgress { done: number; total: number; title: string }

export interface DriveUsage { root: string; totalBytes: number; freeBytes: number }
export interface SystemInfo { elevated: boolean; drive: DriveUsage | null }
