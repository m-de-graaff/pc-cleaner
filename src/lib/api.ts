import { invoke } from "@tauri-apps/api/core";
import type { ExecReport, Finding, JournalEntry, SystemInfo } from "./types";

export const api = {
  scanJunk: () => invoke<Finding[]>("scan_junk"),
  clean: (ids: string[], dryRun: boolean) => invoke<ExecReport>("clean", { ids, dryRun }),
  history: () => invoke<JournalEntry[]>("history"),
  systemInfo: () => invoke<SystemInfo>("system_info"),
  relaunchAsAdmin: () => invoke<void>("relaunch_as_admin"),
};
