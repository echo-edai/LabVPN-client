import { ArrowRight, RefreshCw, Sparkles } from "lucide-react";
import { Link } from "react-router-dom";

import { ConnectHero } from "@/components/features/ConnectHero";
import { DiagnosticsPanel } from "@/components/features/DiagnosticsPanel";
import { ProxyGroups } from "@/components/features/ProxyGroups";
import { RecentLogs } from "@/components/features/RecentLogs";
import { CampusCard, SshCard, SubscriptionCard, VpnStatusCard } from "@/components/features/StatusCards";
import { TrafficChart } from "@/components/features/TrafficChart";
import { PageHeader } from "@/components/layout/PageHeader";
import { Button, Card } from "@/components/ui";
import { ROUTES } from "@/lib/constants";
import { useAppStore } from "@/stores/app";

export function Dashboard() {
  const t = useAppStore((state) => state.t);
  const config = useAppStore((state) => state.config);
  const refresh = useAppStore((state) => state.refresh);
  const busy = useAppStore((state) => state.busy);

  const firstRun = config ? !config.ui.setupCompleted : false;

  return (
    <>
      <PageHeader
        eyebrow="LabVPN"
        title={t("dashboard.title")}
        subtitle={t("dashboard.subtitle")}
        actions={
          <Button
            size="sm"
            variant="secondary"
            icon={<RefreshCw size={13} className={busy ? "animate-spin" : ""} />}
            onClick={() => void refresh()}
          >
            {t("common.refresh")}
          </Button>
        }
      />

      {firstRun ? (
        <Card className="mb-5 flex items-center gap-4 border-[var(--brand)]/25 bg-[var(--brand-soft)] p-4">
          <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-gradient-to-br from-[var(--brand)] to-[var(--accent)] text-white">
            <Sparkles size={18} />
          </div>
          <div className="min-w-0 flex-1">
            <div className="text-[13px] font-semibold text-ink">{t("dashboard.finishSetup")}</div>
            <div className="text-[11.5px] text-ink-muted">{t("dashboard.finishSetupHint")}</div>
          </div>
          <Link to={ROUTES.setupSubscription}>
            <Button variant="primary" size="sm" icon={<ArrowRight size={13} />}>
              {t("nav.firstRun")}
            </Button>
          </Link>
        </Card>
      ) : null}

      <div className="space-y-5">
        <div className="grid grid-cols-1 gap-5 xl:grid-cols-3">
          <div className="xl:col-span-2">
            <ConnectHero />
          </div>
          <CampusCard compact />
        </div>

        <div className="grid grid-cols-1 gap-5 xl:grid-cols-3">
          <div className="xl:col-span-2">
            <TrafficChart />
          </div>
          <div className="space-y-5">
            <VpnStatusCard compact />
            <SubscriptionCard />
          </div>
        </div>

        <div className="grid grid-cols-1 gap-5 xl:grid-cols-3">
          <div className="xl:col-span-2">
            <ProxyGroups limit={5} />
          </div>
          <SshCard />
        </div>

        <div className="grid grid-cols-1 gap-5 xl:grid-cols-3">
          <div className="xl:col-span-2">
            <DiagnosticsPanel />
          </div>
          <RecentLogs />
        </div>
      </div>
    </>
  );
}
