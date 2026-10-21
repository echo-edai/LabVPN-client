import { useEffect } from "react";
import { Navigate, Route, Routes, useNavigate } from "react-router-dom";

import { AppShell } from "@/components/layout/AppShell";
import { Logo } from "@/components/layout/Logo";
import { Spinner } from "@/components/ui";
import { EVENTS, ROUTES } from "@/lib/constants";
import { onEvent } from "@/lib/ipc";
import { Analytics } from "@/pages/Analytics";
import { Dashboard } from "@/pages/Dashboard";
import { Logs } from "@/pages/Logs";
import { Merge } from "@/pages/Merge";
import { Settings } from "@/pages/Settings";
import { SetupCampusVpn } from "@/pages/SetupCampusVpn";
import { SetupSsh } from "@/pages/SetupSsh";
import { SetupSubscription } from "@/pages/SetupSubscription";
import { useAppStore } from "@/stores/app";

function SplashScreen() {
  return (
    <div className="flex h-full w-full flex-col items-center justify-center gap-4">
      <Logo size={76} />
      <div className="text-[15px] font-semibold tracking-tight text-ink">LabVPN</div>
      <Spinner size={18} />
      <div className="text-[11.5px] text-ink-muted">正在加载配置…</div>
    </div>
  );
}

export function App() {
  const init = useAppStore((state) => state.init);
  const ready = useAppStore((state) => state.ready);
  const loading = useAppStore((state) => state.loading);
  const config = useAppStore((state) => state.config);
  const navigate = useNavigate();

  useEffect(() => {
    void init();
  }, [init]);

  useEffect(() => onEvent(EVENTS.navigateSettings, () => navigate(ROUTES.settings)), [navigate]);

  if (loading && !ready) return <SplashScreen />;
  if (!ready) return <SplashScreen />;

  const firstRun = config ? !config.ui.setupCompleted : false;

  return (
    <AppShell>
      <Routes>
        <Route
          path={ROUTES.dashboard}
          element={firstRun ? <Navigate to={ROUTES.setupSubscription} replace /> : <Dashboard />}
        />
        <Route path={ROUTES.analytics} element={<Analytics />} />
        <Route path={ROUTES.setupSubscription} element={<SetupSubscription />} />
        <Route path={ROUTES.setupCampus} element={<SetupCampusVpn />} />
        <Route path={ROUTES.setupSsh} element={<SetupSsh />} />
        <Route path={ROUTES.merge} element={<Merge />} />
        <Route path={ROUTES.logs} element={<Logs />} />
        <Route path={ROUTES.settings} element={<Settings />} />
        <Route path="*" element={<Navigate to={ROUTES.dashboard} replace />} />
      </Routes>
    </AppShell>
  );
}
