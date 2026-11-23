import { ShieldAlert } from "lucide-react";
import { useState } from "react";
import { api } from "../lib/api";

export function AdminBanner() {
  const [error, setError] = useState<string | null>(null);
  return (
    <div className="flex items-center gap-3 border-b border-amber-900/50 bg-amber-950/40 px-8 py-2 text-sm text-amber-200">
      <ShieldAlert size={16} aria-hidden="true" />
      <span className="flex-1">
        Some system locations need administrator rights.
        {error && <span role="alert"> {error}</span>}
      </span>
      <button
        type="button"
        onClick={() => api.relaunchAsAdmin().catch((e) => setError(String(e)))}
        className="rounded-md bg-amber-500/20 px-3 py-1 font-medium hover:bg-amber-500/30 focus-visible:outline-2 focus-visible:outline-amber-300"
      >
        Restart as admin
      </button>
    </div>
  );
}
