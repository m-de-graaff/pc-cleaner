import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { formatBytes } from "../lib/format";
import type { JournalEntry } from "../lib/types";

function outcome(e: JournalEntry): string {
  if (e.dryRun) return `Dry run: ${e.deletedCount} items`;
  const skipped = e.failedCount > 0 ? `, ${e.failedCount} skipped` : "";
  return `${e.deletedCount} removed${skipped}`;
}

export function HistoryPage() {
  const [entries, setEntries] = useState<JournalEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.history().then(setEntries).catch((e) => setError(String(e)));
  }, []);

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-semibold tracking-tight">History</h1>
      {error && <p role="alert" className="text-red-400">Couldn't load history: {error}</p>}
      {!entries && !error && <p className="text-zinc-400">Loading…</p>}
      {entries?.length === 0 && <p className="text-zinc-400">Nothing has been cleaned yet.</p>}
      {entries && entries.length > 0 && (
        <table className="w-full text-left text-sm">
          <thead className="border-b border-zinc-800 text-zinc-400">
            <tr>
              <th scope="col" className="py-2 font-medium">When</th>
              <th scope="col" className="py-2 font-medium">Item</th>
              <th scope="col" className="py-2 font-medium">Result</th>
              <th scope="col" className="py-2 text-right font-medium">Freed</th>
            </tr>
          </thead>
          <tbody>
            {entries.map((e, i) => (
              <tr key={`${e.timestamp}-${e.findingId}-${i}`} className="border-b border-zinc-900">
                <td className="py-2 text-zinc-400">{new Date(e.timestamp * 1000).toLocaleString()}</td>
                <td className="py-2">{e.title}</td>
                <td className={`py-2 ${e.dryRun ? "text-zinc-500" : "text-zinc-300"}`}>{outcome(e)}</td>
                <td className="py-2 text-right tabular-nums">{formatBytes(e.freedBytes)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
