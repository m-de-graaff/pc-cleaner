import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import type { Finding, ScanProgress, ScanResult } from "./types";

export type ScanStatus = "idle" | "scanning" | "done" | "error";

export interface Scan {
  findings: Finding[];
  /** Messages that aren't findings, e.g. "Docker isn't running". */
  notices: string[];
  status: ScanStatus;
  error: string | null;
  /** Latest progress event while scanning; null before the first location finishes. */
  progress: ScanProgress | null;
  scan: () => Promise<void>;
}

/** Scan state for one page. `fetch` runs the backend scan. */
export function useScan(fetch: () => Promise<ScanResult>): Scan {
  const [result, setResult] = useState<ScanResult>({ findings: [], notices: [] });
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
      setResult(await fetch());
      setStatus("done");
    } catch (e) {
      setError(String(e));
      setStatus("error");
    }
  }, [fetch]);

  return { ...result, status, error, progress, scan };
}
