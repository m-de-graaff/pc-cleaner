import { useCallback, useEffect, useState } from "react";
import { AdminBanner } from "./components/AdminBanner";
import { Sidebar, type Page } from "./components/Sidebar";
import { api } from "./lib/api";
import type { SystemInfo } from "./lib/types";
import { useJunkScan } from "./lib/useJunkScan";
import { Dashboard } from "./pages/Dashboard";
import { HistoryPage } from "./pages/HistoryPage";
import { JunkPage } from "./pages/JunkPage";

export default function App() {
  const [page, setPage] = useState<Page>("dashboard");
  const [info, setInfo] = useState<SystemInfo | null>(null);
  const scan = useJunkScan();

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
          {page === "dashboard" && <Dashboard info={info} scan={scan} onReview={() => setPage("junk")} />}
          {page === "junk" && <JunkPage scan={scan} elevated={elevated} onCleaned={refreshInfo} />}
          {page === "history" && <HistoryPage />}
        </div>
      </main>
    </div>
  );
}
