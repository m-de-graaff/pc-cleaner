import { useCallback, useState } from "react";
import { api } from "./api";
import type { Finding } from "./types";

export type ScanStatus = "idle" | "scanning" | "done" | "error";

export interface JunkScan {
  findings: Finding[];
  status: ScanStatus;
  error: string | null;
  scan: () => Promise<void>;
}

export function useJunkScan(): JunkScan {
  const [findings, setFindings] = useState<Finding[]>([]);
  const [status, setStatus] = useState<ScanStatus>("idle");
  const [error, setError] = useState<string | null>(null);

  const scan = useCallback(async () => {
    setStatus("scanning");
    setError(null);
    try {
      setFindings(await api.scanJunk());
      setStatus("done");
    } catch (e) {
      setError(String(e));
      setStatus("error");
    }
  }, []);

  return { findings, status, error, scan };
}
