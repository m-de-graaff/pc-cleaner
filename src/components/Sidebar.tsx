import { Container, History, LayoutDashboard, Trash2 } from "lucide-react";

export type Page = "dashboard" | "junk" | "devtools" | "history";

const ITEMS = [
  { id: "dashboard", label: "Overview", icon: LayoutDashboard },
  { id: "junk", label: "Junk & temp files", icon: Trash2 },
  { id: "devtools", label: "Dev tools", icon: Container },
  { id: "history", label: "History", icon: History },
] as const;

export function Sidebar({ page, onNavigate }: { page: Page; onNavigate: (p: Page) => void }) {
  return (
    <nav aria-label="Main" className="flex w-60 shrink-0 flex-col gap-1 border-r border-zinc-800 bg-zinc-900/60 p-4">
      <div className="mb-6 px-3 text-lg font-semibold tracking-tight">PC Cleaner</div>
      {ITEMS.map(({ id, label, icon: Icon }) => (
        <button
          key={id}
          type="button"
          onClick={() => onNavigate(id)}
          aria-current={page === id ? "page" : undefined}
          className={`flex items-center gap-3 rounded-lg px-3 py-2 text-left text-sm transition-colors focus-visible:outline-2 focus-visible:outline-sky-400 ${
            page === id ? "bg-zinc-800 text-white" : "text-zinc-400 hover:bg-zinc-800/60 hover:text-zinc-100"
          }`}
        >
          <Icon size={18} aria-hidden="true" />
          {label}
        </button>
      ))}
    </nav>
  );
}
