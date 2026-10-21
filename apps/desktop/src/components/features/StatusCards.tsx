import {
  ArrowUpRight,
  CloudDownload,
  GraduationCap,
  RefreshCw,
  Server,
  TerminalSquare,
  Wifi,
  WifiOff,
} from "lucide-react";
import { Link } from "react-router-dom";

import { Badge, Button, Card, CardHeader, Progress, Row, StatusDot } from "@/components/ui";
import { ROUTES } from "@/lib/constants";
import { cn } from "@/lib/cn";
import { formatBytes, relativeTime, trafficUsage } from "@/lib/format";
import { useAppStore } from "@/stores/app";

export function CampusCard({ compact = false }: { compact?: boolean }) {
  const t = useAppStore((state) => state.t);
  const campus = useAppStore((state) => state.campus);
  const config = useAppStore((state) => state.config);
  const refreshCampus = useAppStore((state) => state.refreshCampus);
  const setCampusOverride = useAppStore((state) => state.setCampusOverride);
  const system = useAppStore((state) => state.system);

  const onCampus = campus?.onCampus ?? false;
  const override = system?.campusOverride ?? null;
  const pingHost =
    config?.campusVpn.probeTargets.find((target) => target.protocol === "icmp")?.host ??
    "10.134.140.1";
  const pingSignal = campus?.signals.find((signal) => signal.kind === "pingProbe");
  const matchedSignals = (campus?.signals ?? []).filter((signal) => signal.ok);
  const basis = pingSignal?.ok
    ? `Ping ${pingHost} 可达`
    : matchedSignals.length > 0
      ? matchedSignals
          .slice(0, 2)
          .map((signal) => `${signal.name}`)
          .join(" · ")
      : `Ping ${pingHost} 不可达，其他校内依据也未命中`;

  return (
    <Card>
      <CardHeader
        title={t("dashboard.campusCard")}
        subtitle={system?.wifiSsid ?? system?.defaultInterface ?? "—"}
        icon={<Wifi size={15} />}
        actions={
          <Button
            size="sm"
            variant="ghost"
            icon={<RefreshCw size={12} />}
            onClick={() => void refreshCampus()}
          >
            {t("setupCampus.refreshDetect")}
          </Button>
        }
      />
      <div className="flex items-start gap-2.5">
        <div
          className={cn(
            "mt-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border",
            onCampus
              ? "border-[var(--ok)]/25 bg-[var(--ok)]/10 text-[var(--ok)]"
              : "border-line bg-card text-ink-muted",
          )}
        >
          {onCampus ? <Wifi size={17} /> : <WifiOff size={17} />}
        </div>
        <div className="min-w-0 flex-1">
          <div className="text-[15px] font-semibold leading-5 text-ink">
            {onCampus ? t("status.onCampus") : t("status.offCampus")}
          </div>
          <div className="mt-0.5 break-words text-[11px] leading-4 text-ink-muted">
            {campus ? `${t("campusCard.basis")}：${basis}` : t("status.campusUnknown")}
          </div>
        </div>
      </div>
      <div className="mt-2.5 flex items-center gap-1">
        <span className="mr-0.5 text-[10px] text-ink-muted">手动</span>
        {([
          { value: null, label: "自动检测" },
          { value: true, label: "强制校内" },
          { value: false, label: "强制校外" },
        ] as const).map((item) => (
          <button
            key={String(item.value)}
            type="button"
            onClick={() => void setCampusOverride(item.value)}
            className={cn(
              "rounded-lg px-2 py-1 text-[10.5px] transition",
              override === item.value
                ? "bg-[var(--brand-soft)] text-brand"
                : "text-ink-muted hover:bg-card hover:text-ink",
            )}
          >
            {item.label}
          </button>
        ))}
      </div>

      {campus ? (
        <>
          <div className="mt-3 flex items-start justify-between gap-2 rounded-lg border border-line bg-card px-2.5 py-2 text-[10.5px] leading-4 text-ink-muted">
            <span>{t("campusCard.autoSwitch")}</span>
            <Link to={ROUTES.setupCampus} className="shrink-0 text-brand hover:underline">
              {t("campusCard.editBasis")}
            </Link>
          </div>
          <div className="mt-2.5 max-h-48 space-y-1 overflow-y-auto scroll-slim pr-1">
            {[...campus.signals]
              .sort((a, b) => Number(b.ok) - Number(a.ok))
              .filter((signal) => (compact ? signal.ok : true))
              .slice(0, compact ? 4 : 10)
              .map((signal, index) => (
                <div key={`${signal.name}-${index}`} className="flex items-start gap-2 rounded-lg px-1 py-1">
                  <StatusDot tone={signal.ok ? "ok" : "muted"} />
                  <div className="min-w-0 flex-1">
                    <div className="text-[11px] leading-4 text-ink-soft">{signal.name}</div>
                    <div className="break-words text-[10px] leading-4 text-ink-muted">{signal.detail}</div>
                  </div>
                </div>
              ))}
            {compact && campus.signals.every((signal) => !signal.ok) ? (
              <div className="text-[10.5px] text-ink-muted">未命中任何校内依据</div>
            ) : null}
          </div>
        </>
      ) : (
        <div className="mt-4 text-[12px] text-ink-muted">{t("dashboard.noCampusData")}</div>
      )}
    </Card>
  );
}

export function VpnStatusCard({ compact = false }: { compact?: boolean }) {
  const t = useAppStore((state) => state.t);
  const vpn = useAppStore((state) => state.vpn);
  const testVpn = useAppStore((state) => state.testVpn);
  const busy = useAppStore((state) => state.busy);

  const tone =
    vpn?.state === "running" || vpn?.state === "reusing"
      ? "ok"
      : vpn?.state === "error"
        ? "err"
        : vpn?.state === "starting"
          ? "warn"
          : "muted";

  const stateLabel = {
    running: t("status.vpnRunning"),
    reusing: t("status.vpnReusing"),
    starting: t("status.vpnStopped"),
    stopped: t("status.vpnStopped"),
    disabled: t("status.vpnDisabled"),
    error: t("status.error"),
    detecting: t("status.idle"),
  }[vpn?.state ?? "stopped"];

  return (
    <Card>
      <CardHeader
        title={t("dashboard.vpnCard")}
        subtitle="fdu-connect"
        icon={<GraduationCap size={15} />}
        actions={
          <Badge tone={tone}>
            <StatusDot tone={tone} pulse={vpn?.state === "running"} />
            {stateLabel}
          </Badge>
        }
      />
      <Row label="模式" value={vpn?.managed ? "托管" : "复用已有实例"} />
      <Row label="SOCKS" value={vpn ? `127.0.0.1:${vpn.socksPort}` : "—"} mono />
      {!compact ? <Row label="可执行文件" value={vpn?.binaryPath ?? "未检测到"} mono /> : null}
      {vpn?.lastError ? (
        <div className="mt-2 rounded-lg border border-[var(--err)]/20 bg-[var(--err)]/10 px-2.5 py-1.5 text-[11px] text-[var(--err)]">
          {vpn.lastError}
        </div>
      ) : null}
      <div className="mt-3 flex items-center justify-between">
        <Link to={ROUTES.setupCampus} className="text-[11.5px] text-brand hover:underline">
          {t("nav.setupCampus")} →
        </Link>
        <Button size="sm" variant="ghost" loading={busy === "vpn-test"} onClick={() => void testVpn()}>
          {t("setupCampus.test")}
        </Button>
      </div>
    </Card>
  );
}

export function SubscriptionCard(_props: { compact?: boolean }) {
  const t = useAppStore((state) => state.t);
  const subscription = useAppStore((state) => state.subscription);
  const refreshSubscription = useAppStore((state) => state.refreshSubscription);
  const busy = useAppStore((state) => state.busy);
  const usage = trafficUsage(subscription?.traffic ?? null);

  return (
    <Card>
      <CardHeader
        title={t("dashboard.subscriptionCard")}
        subtitle={subscription?.name ?? "—"}
        icon={<CloudDownload size={15} />}
        actions={
          <Button
            size="sm"
            variant="ghost"
            icon={<RefreshCw size={12} />}
            loading={busy === "subscription"}
            onClick={() => void refreshSubscription()}
          >
            {t("common.refresh")}
          </Button>
        }
      />
      <div className="flex items-end justify-between">
        <div>
          <div className="text-[22px] font-semibold text-ink">{subscription?.proxyCount ?? 0}</div>
          <div className="text-[11px] text-ink-muted">{t("setupSubscription.nodeCount")}</div>
        </div>
        <Badge tone={subscription?.enabled ? "brand" : "muted"}>
          {subscription?.enabled ? t("common.enabled") : t("common.disabled")}
        </Badge>
      </div>
      {usage ? (
        <div className="mt-4">
          <div className="mb-1.5 flex items-center justify-between text-[11px]">
            <span className="text-ink-muted">{t("setupSubscription.usage")}</span>
            <span className="text-ink-soft">
              {formatBytes(usage.used)}
              <span className="text-ink-muted"> / {usage.total > 0 ? formatBytes(usage.total) : "∞"}</span>
            </span>
          </div>
          <Progress ratio={usage.ratio} tone={usage.ratio > 0.9 ? "err" : usage.ratio > 0.75 ? "warn" : "brand"} />
          <div className="mt-1.5 text-[10.5px] text-ink-muted">{usage.expireText}</div>
        </div>
      ) : null}
      <div className="mt-3 flex items-center justify-between">
        <Link to={ROUTES.setupSubscription} className="text-[11.5px] text-brand hover:underline">
          {t("nav.setupSubscription")} →
        </Link>
        <span className="text-[10.5px] text-ink-muted">
          {relativeTime(subscription?.lastUpdated ?? null)}
        </span>
      </div>
    </Card>
  );
}

export function SshCard() {
  const t = useAppStore((state) => state.t);
  const config = useAppStore((state) => state.config);
  const ssh = config?.sshProxy;

  return (
    <Card>
      <CardHeader
        title={t("dashboard.sshCard")}
        subtitle="mihomo ssh outbound"
        icon={<TerminalSquare size={15} />}
        actions={
          <Badge tone={ssh?.enabled ? "brand" : "muted"}>
            {ssh?.enabled ? t("common.enabled") : t("common.disabled")}
          </Badge>
        }
      />
      {ssh?.enabled ? (
        <>
          <Row label="服务器" value={`${ssh.host}:${ssh.port}`} mono />
          <Row label="用户名" value={ssh.username || "—"} mono />
          <Row
            label="认证"
            value={ssh.auth === "password" ? t("setupSsh.password") : t("setupSsh.privateKey")}
          />
          <div className="mt-2 flex flex-wrap gap-1">
            {ssh.routes.slice(0, 4).map((route) => (
              <span key={route.value} className="chip font-mono text-[10.5px]">
                {route.value}
              </span>
            ))}
            {ssh.routes.length === 0 ? (
              <span className="text-[11px] text-ink-muted">{t("common.notConfigured")}</span>
            ) : null}
          </div>
        </>
      ) : (
        <div className="flex items-center gap-2 py-2 text-[12px] text-ink-muted">
          <Server size={14} />
          {t("setupSsh.skipHint")}
        </div>
      )}
      <div className="mt-3 flex items-center justify-between">
        <Link to={ROUTES.setupSsh} className="text-[11.5px] text-brand hover:underline">
          {t("nav.setupSsh")} →
        </Link>
        <ArrowUpRight size={13} className="text-ink-muted" />
      </div>
    </Card>
  );
}
