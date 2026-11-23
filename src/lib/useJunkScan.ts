import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import type { Finding, ScanProgress } from "./types";

export type ScanStatus = "idle" | "scanning" | "done" | "error";

export interface JunkScan {
  findings: Finding[];
  status: ScanStatus;
  error: string | null;
  /** Latest progress event while scanning; null before the first location finishes. */
  progress: ScanProgress | null;
  scan: () => Promise<void>;
}

export function useJunkScan(): JunkScan {
  const [findings, setFindings] = useState<Finding[]>([]);
  const [status, setStatus] = useState<ScanStatus>("idle");
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<ScanProgress | null>(null);

  useEffect(() => {
    const unlisten = listen<ScanProgress>("scan-progress", (e) => setProgress(e.payload));
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, []);

  const scan = useCallback(async () => {
    setStatus("scanning");
    setError(null);
    setProgress(null);
    try {
      setFindings(await api.scanJunk());
      setStatus("done");
    } catch (e) {
      setError(String(e));
      setStatus("error");
    }
  }, []);

  return { findings, status, error, progress, scan };
}
