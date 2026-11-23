import { AlertTriangle, ShieldAlert } from "lucide-react";
import { useId } from "react";
import { formatBytes } from "../lib/format";
import type { Finding } from "../lib/types";

interface Props { finding: Finding; checked: boolean; disabled: boolean; onToggle: () => void }

export function FindingRow({ finding: f, checked, disabled, onToggle }: Props) {
  const id = useId();
  return (
    <li className={`flex gap-4 rounded-lg border border-zinc-800 bg-zinc-900 p-4 ${disabled ? "opacity-60" : ""}`}>
      <input
        id={id}
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={onToggle}
        aria-describedby={`${id}-reason`}
        className="mt-1 size-4 accent-sky-500"
      />
      <label htmlFor={id} className="flex-1 cursor-pointer">
        <span className="flex items-center gap-2">
          <span className="font-medium">{f.title}</span>
          {f.requiresAdmin && (
            <span className="inline-flex items-center gap-1 rounded bg-amber-500/15 px-1.5 py-0.5 text-xs text-amber-300">
              <ShieldAlert size={12} aria-hidden="true" /> Admin
            </span>
          )}
          {!f.reversible && f.risk !== "safe" && (
            <span className="inline-flex items-center gap-1 rounded bg-red-500/15 px-1.5 py-0.5 text-xs text-red-300">
              <AlertTriangle size={12} aria-hidden="true" /> Can't be undone
            </span>
          )}
          <span className="ml-auto tabular-nums text-zinc-300">{formatBytes(f.sizeBytes)}</span>
        </span>
        <span id={`${id}-reason`} className="mt-1 block text-sm text-zinc-400">{f.reason}</span>
      </label>
    </li>
  );
}
