import { HardDrive, Loader2, Sparkles } from "lucide-react";
import { formatBytes } from "../lib/format";
import type { SystemInfo } from "../lib/types";
import type { JunkScan } from "../lib/useJunkScan";

const primary =
  "inline-flex items-center gap-2 rounded-lg bg-sky-500 px-4 py-2 font-medium text-zinc-950 hover:bg-sky-400 disabled:opacity-60 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-sky-300";
const secondary =
  "inline-flex items-center gap-2 rounded-lg border border-zinc-700 px-4 py-2 text-zinc-200 hover:bg-zinc-800 disabled:opacity-60 focus-visible:outline-2 focus-visible:outline-sky-400";

export function Dashboard({ info, scan, onReview }: { info: SystemInfo | null; scan: JunkScan; onReview: () => void }) {
  const drive = info?.drive;
  const usedPct = drive ? Math.round(((drive.totalBytes - drive.freeBytes) / drive.totalBytes) * 100) : 0;
  const total = scan.findings.reduce((s, f) => s + f.sizeBytes, 0);
  const safe = scan.findings.filter((f) => f.risk === "safe").reduce((s, f) => s + f.sizeBytes, 0);
  const scanning = scan.status === "scanning";

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-semibold tracking-tight">Overview</h1>

      {drive && (
        <section aria-labelledby="drive-heading" className="rounded-xl border border-zinc-800 bg-zinc-900 p-5">
          <div className="mb-3 flex items-center gap-2">
            <HardDrive size={18} aria-hidden="true" />
            <h2 id="drive-heading" className="font-medium">Drive {drive.root}</h2>
            <span className="ml-auto text-sm tabular-nums text-zinc-400">
              {formatBytes(drive.freeBytes)} free of {formatBytes(drive.totalBytes)}
            </span>
          </div>
          <div
            role="progressbar"
            aria-label="Disk space used"
            aria-valuenow={usedPct}
            aria-valuemin={0}
            aria-valuemax={100}
            className="h-3 overflow-hidden rounded-full bg-zinc-800"
          >
            <div
              className={`h-full rounded-full ${usedPct > 90 ? "bg-red-500" : usedPct > 75 ? "bg-amber-500" : "bg-sky-500"}`}
              style={{ width: `${usedPct}%` }}
            />
          </div>
        </section>
      )}

      <section aria-labelledby="junk-heading" className="rounded-xl border border-zinc-800 bg-zinc-900 p-5">
        <h2 id="junk-heading" className="sr-only">Junk files</h2>
        {scan.status === "done" ? (
          <div className="flex items-center gap-4">
            <div className="flex-1">
              <p className="text-3xl font-semibold tabular-nums">{formatBytes(total)}</p>
              <p className="text-sm text-zinc-400">
                can be freed. {formatBytes(safe)} of it is safe to remove right away.
              </p>
            </div>
            <button type="button" className={secondary} onClick={scan.scan}>Scan again</button>
            <button type="button" className={primary} onClick={onReview}>Review</button>
          </div>
        ) : (
          <div className="flex items-center gap-4">
            <p className="flex-1 text-zinc-300">Find temporary files, caches and other junk that's safe to remove.</p>
            <button type="button" className={primary} disabled={scanning} onClick={scan.scan}>
              {scanning ? <Loader2 size={16} className="animate-spin" aria-hidden="true" /> : <Sparkles size={16} aria-hidden="true" />}
              {scanning ? "Scanning…" : "Scan now"}
            </button>
          </div>
        )}
        {scan.error && <p role="alert" className="mt-3 text-sm text-red-400">The scan failed: {scan.error}</p>}
      </section>
    </div>
  );
}
