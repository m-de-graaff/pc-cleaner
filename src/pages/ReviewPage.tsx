import { Loader2, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { FindingRow } from "../components/FindingRow";
import { api } from "../lib/api";
import { formatBytes, progressLabel } from "../lib/format";
import { confirmNotes, defaultSelection, groupByRisk, isSelectable, RISK_ORDER, selectedTotal } from "../lib/selection";
import type { ExecReport, Risk } from "../lib/types";
import type { Scan } from "../lib/useScan";

const GROUP_COPY: Record<Risk, { heading: string; hint: string }> = {
  safe: { heading: "Safe to remove", hint: "Windows and your apps recreate these automatically." },
  review: { heading: "Review first", hint: "Usually fine to remove, but take a look before you do." },
  caution: { heading: "Be careful", hint: "Only remove these if you're sure." },
};

interface Props { title: string; intro: string; scan: Scan; elevated: boolean; onCleaned: () => void }
interface Result { report: ExecReport; titles: Record<string, string> }

export function ReviewPage({ title, intro, scan, elevated, onCleaned }: Props) {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [dryRun, setDryRun] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<Result | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (scan.status === "idle") void scan.scan();
  }, [scan.status, scan.scan]);
  useEffect(() => {
    setSelected(defaultSelection(scan.findings, elevated));
  }, [scan.findings, elevated]);

  const groups = groupByRisk(scan.findings);
  const total = selectedTotal(scan.findings, selected);
  const notes = confirmNotes(scan.findings, selected);
  const hasCompaction = scan.findings.some((f) => f.upperBound && selected.has(f.id));
  const toggle = (id: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  async function runClean() {
    setConfirming(false);
    setBusy(true);
    setError(null);
    const titles = Object.fromEntries(scan.findings.map((f) => [f.id, f.title]));
    try {
      const report = await api.clean([...selected], dryRun);
      setResult({ report, titles });
      if (!dryRun) {
        await scan.scan();
        onCleaned();
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const problems = result?.report.items.filter((i) => i.skipped || i.failedCount > 0) ?? [];

  return (
    <div className="space-y-6 pb-24">
      <div className="flex items-center gap-3">
        <h1 className="flex-1 text-2xl font-semibold tracking-tight">{title}</h1>
        <button type="button" onClick={scan.scan} disabled={scan.status === "scanning" || busy}
          className="inline-flex items-center gap-2 rounded-lg border border-zinc-700 px-3 py-1.5 text-sm hover:bg-zinc-800 disabled:opacity-60 focus-visible:outline-2 focus-visible:outline-sky-400">
          <RefreshCw size={14} aria-hidden="true" /> Scan again
        </button>
      </div>

      <p className="text-zinc-400">{intro}</p>

      {scan.notices.map((n) => (
        <p key={n} role="status" className="rounded-lg border border-amber-900/50 bg-amber-950/30 px-4 py-3 text-sm text-amber-200">{n}</p>
      ))}

      {scan.status === "scanning" && (
        <p role="status" className="flex items-center gap-2 text-zinc-400">
          <Loader2 size={16} className="animate-spin" aria-hidden="true" /> {progressLabel(scan.progress)}
        </p>
      )}
      {scan.error && <p role="alert" className="text-red-400">The scan failed: {scan.error}</p>}
      {error && <p role="alert" className="text-red-400">Cleaning failed: {error}</p>}

      {result && (
        <section role="status" className="rounded-xl border border-emerald-900/60 bg-emerald-950/30 p-4">
          <p className="font-medium text-emerald-200">
            {result.report.dryRun ? "Dry run: this would free " : "Freed "}
            <span className="tabular-nums">{formatBytes(result.report.totalFreed)}</span>.
          </p>
          {problems.length > 0 && (
            <ul className="mt-2 space-y-1 text-sm text-zinc-300">
              {problems.map((i) => (
                <li key={i.findingId}>
                  {result.titles[i.findingId] ?? i.findingId}:{" "}
                  {i.skipped ?? (i.failedCount === 1 && i.errors[0] ? i.errors[0] : `${i.failedCount} files were in use and were skipped.`)}
                </li>
              ))}
            </ul>
          )}
        </section>
      )}

      {scan.status === "done" && scan.findings.length === 0 && (
        <p className="text-zinc-400">Nothing to clean here. Everything this page checks is already clean.</p>
      )}

      {RISK_ORDER.filter((r) => groups[r].length > 0).map((risk) => (
        <section key={risk} aria-labelledby={`group-${risk}`} className="space-y-2">
          <h2 id={`group-${risk}`} className="font-medium">{GROUP_COPY[risk].heading}</h2>
          <p className="text-sm text-zinc-400">{GROUP_COPY[risk].hint}</p>
          <ul className="space-y-2">
            {groups[risk].map((f) => (
              <FindingRow key={f.id} finding={f} checked={selected.has(f.id)}
                disabled={!isSelectable(f, elevated) || busy} onToggle={() => toggle(f.id)} />
            ))}
          </ul>
        </section>
      ))}

      <footer className="fixed bottom-0 left-60 right-0 flex items-center gap-4 border-t border-zinc-800 bg-zinc-950/95 px-8 py-4 backdrop-blur">
        <label className="flex items-center gap-2 text-sm text-zinc-300">
          <input type="checkbox" checked={dryRun} onChange={(e) => setDryRun(e.target.checked)} className="size-4 accent-sky-500" />
          Dry run (show what would be removed)
        </label>
        <span className="ml-auto text-sm tabular-nums text-zinc-300">
          {selected.size} selected · {formatBytes(total)}{hasCompaction && " + disk compaction"}
        </span>
        <button type="button" disabled={selected.size === 0 || busy} onClick={() => setConfirming(true)}
          className="inline-flex items-center gap-2 rounded-lg bg-sky-500 px-4 py-2 font-medium text-zinc-950 hover:bg-sky-400 disabled:opacity-50 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-sky-300">
          {busy && <Loader2 size={16} className="animate-spin" aria-hidden="true" />}
          {dryRun ? "Run dry run" : "Clean selected"}
        </button>
      </footer>

      <ConfirmDialog
        open={confirming}
        title={dryRun ? "Run a dry run?" : total > 0 ? `Remove ${formatBytes(total)}?` : `Clean ${selected.size} items?`}
        confirmLabel={dryRun ? "Run dry run" : "Remove permanently"}
        onConfirm={runClean}
        onCancel={() => setConfirming(false)}
      >
        {dryRun ? (
          "Nothing will be deleted and no commands run. You'll see how much space would be freed."
        ) : (
          <>
            <p>{selected.size} items will be removed permanently. Nothing here goes to the Recycle Bin, because that would free no space.</p>
            {notes.map((n) => <p key={n} className="mt-2 font-medium text-amber-200">{n}</p>)}
          </>
        )}
      </ConfirmDialog>
    </div>
  );
}
