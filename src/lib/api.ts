import { invoke } from "@tauri-apps/api/core";
import type { ExecReport, Finding, JournalEntry, ScanResult, SystemInfo } from "./types";

export const api = {
  scanJunk: async (): Promise<ScanResult> => ({ findings: await invoke<Finding[]>("scan_junk"), notices: [] }),
  scanDevtools: () => invoke<ScanResult>("scan_devtools"),
  clean: (ids: string[], dryRun: boolean) => invoke<ExecReport>("clean", { ids, dryRun }),
  history: () => invoke<JournalEntry[]>("history"),
  systemInfo: () => invoke<SystemInfo>("system_info"),
  relaunchAsAdmin: () => invoke<void>("relaunch_as_admin"),
};
