import { useCallback, useEffect, useState } from "react";
import { AdminBanner } from "./components/AdminBanner";
import { Sidebar, type Page } from "./components/Sidebar";
import { api } from "./lib/api";
import type { SystemInfo } from "./lib/types";
import { useScan } from "./lib/useScan";
import { Dashboard } from "./pages/Dashboard";
import { HistoryPage } from "./pages/HistoryPage";
import { ReviewPage } from "./pages/ReviewPage";

export default function App() {
  const [page, setPage] = useState<Page>("dashboard");
  const [info, setInfo] = useState<SystemInfo | null>(null);
  const junk = useScan(api.scanJunk);
  const devtools = useScan(api.scanDevtools);

  const refreshInfo = useCallback(() => {
    api.systemInfo().then(setInfo).catch(() => setInfo(null));
  }, []);
  useEffect(refreshInfo, [refreshInfo]);

  const elevated = info?.elevated ?? false;

  return (
    <div className="flex h-screen bg-zinc-950 text-zinc-100">
      <Sidebar page={page} onNavigate={setPage} />
      <main className="flex-1 overflow-y-auto">
        {info && !elevated && <AdminBanner />}
        <div className="mx-auto max-w-4xl p-8">
          {page === "dashboard" && <Dashboard info={info} scan={junk} onReview={() => setPage("junk")} />}
          {page === "junk" && (
            <ReviewPage
              title="Junk & temp files"
              intro="Temporary files, caches and leftovers that Windows, your browsers and your tools recreate when needed."
              scan={junk}
              elevated={elevated}
              onCleaned={refreshInfo}
            />
          )}
          {page === "devtools" && (
            <ReviewPage
              title="Dev tools"
              intro="Docker data, WSL disks that can be compacted, caches inside your Linux distros, Rust toolchains you don't use by default, and Android emulators. Scanning briefly starts your WSL distros to measure them."
              scan={devtools}
              elevated={elevated}
              onCleaned={refreshInfo}
            />
          )}
          {page === "history" && <HistoryPage />}
        </div>
      </main>
    </div>
  );
}
