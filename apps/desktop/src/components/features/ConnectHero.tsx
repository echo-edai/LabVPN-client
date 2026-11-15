import { Activity, Clock3, Globe2, Power, RefreshCw, ShieldCheck, Wifi } from "lucide-react";

import { Badge, Button, Card, StatusDot } from "@/components/ui";
import { cn } from "@/lib/cn";
import { formatDuration } from "@/lib/format";
import { useAppStore } from "@/stores/app";

export function ConnectHero() {
  const t = useAppStore((state) => state.t);
  const engine = useAppStore((state) => state.engine);
  const campus = useAppStore((state) => state.campus);
  const busy = useAppStore((state) => state.busy);
  const connect = useAppStore((state) => state.connect);
  const disconnect = useAppStore((state) => state.disconnect);
  const restart = useAppStore((state) => state.restart);
  const system = useAppStore((state) => state.system);

  const running = engine?.running ?? false;
  const connecting = busy === "connect" || busy === "restart" || engine?.state === "starting";
  const operationBusy = Boolean(busy) || engine?.state === "starting";
  const warnings = [...new Set([...(engine?.warnings ?? []), ...(system?.proxyWarnings ?? [])])];
  const uptime = engine?.startedAt
    ? Math.max(0, Math.floor((Date.now() - new Date(engine.startedAt).getTime()) / 1000))
    : 0;

  return (
    <Card className="relative overflow-hidden p-0">
      <div className="pointer-events-none absolute -right-20 -top-28 h-72 w-72 rounded-full bg-[var(--brand)]/20 blur-3xl" />
      <div className="pointer-events-none absolute -bottom-32 left-1/3 h-64 w-64 rounded-full bg-[var(--accent)]/10 blur-3xl" />
      <div className="relative flex flex-col-reverse items-center gap-7 p-6 md:flex-row md:items-center md:p-7">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <Badge tone={running ? "ok" : "muted"} icon={<StatusDot tone={running ? "ok" : "muted"} pulse={running} />}>
              {running ? t("status.connected") : t("status.disconnected")}
            </Badge>
            {engine?.mode ? (
              <Badge tone="brand" icon={<Activity size={11} />}>
                {engine.mode === "tun" ? t("mode.tun") : t("mode.systemProxy")}
              </Badge>
            ) : null}
            {campus ? (
              <Badge tone={campus.onCampus ? "info" : "muted"} icon={<Wifi size={11} />}>
                {campus.onCampus ? t("status.onCampus") : t("status.offCampus")}
              </Badge>
            ) : null}
          </div>
          <h2 className="mt-3 text-[20px] font-semibold tracking-tight text-ink">
            {running ? t("dashboard.heroRunning") : t("dashboard.heroIdle")}
          </h2>
          <p className="mt-1.5 max-w-lg text-[12.5px] leading-5 text-ink-muted">{t("dashboard.heroDesc")}</p>
          {running ? (
            <p className="mt-2 max-w-lg text-sm leading-5 text-ink-soft" role="status">
              {t("dashboard.healthHint")}
            </p>
          ) : null}

          <div className="mt-5 flex flex-wrap items-center gap-x-5 gap-y-2 text-[11.5px] text-ink-muted">
            <span className="inline-flex items-center gap-1.5">
              <Globe2 size={13} />
              {running && engine?.helperMode
                ? t("status.helperRunning")
                : system
                  ? t(system.systemProxyEnabled ? "status.systemProxyOn" : "status.systemProxyOff")
                  : t("settings.proxyStatusUnknown")}
            </span>
            <span className="inline-flex items-center gap-1.5">
              <Clock3 size={13} />
              {running ? `${t("dashboard.uptime")} ${formatDuration(uptime)}` : "—"}
            </span>
            <span className="inline-flex items-center gap-1.5">
              <ShieldCheck size={13} />
              {engine?.mixedPort ? `127.0.0.1:${engine.mixedPort}` : "—"}
            </span>
          </div>

          <div className="mt-5 flex flex-wrap items-center gap-2">
            {running ? (
              <Button
                variant="danger"
                onClick={() => void disconnect()}
                disabled={operationBusy}
                loading={busy === "disconnect"}
              >
                {t("common.stop")}
              </Button>
            ) : (
              <Button
                variant="primary"
                onClick={() => void connect()}
                disabled={operationBusy}
                loading={connecting}
                icon={<Power size={15} />}
              >
                {t("common.start")}
              </Button>
            )}
            <Button
              variant="secondary"
              onClick={() => void restart()}
              disabled={!engine?.running || operationBusy}
              loading={busy === "restart"}
              icon={<RefreshCw size={14} />}
            >
              {t("common.restart")}
            </Button>
          </div>
          {running ? <p className="mt-2 text-xs leading-5 text-ink-muted">{t("dashboard.reconnectHint")}</p> : null}
          {warnings.length ? (
            <div
              role="status"
              className="mt-3 rounded-lg border border-[var(--warn)]/30 bg-[var(--warn)]/10 p-3 text-sm leading-5 text-ink-soft"
            >
              <p className="font-medium">{t("dashboard.proxyWarnings")}</p>
              <ul className="mt-1 list-disc space-y-1 pl-4">
                {warnings.map((warning) => (
                  <li key={warning}>{warning}</li>
                ))}
              </ul>
            </div>
          ) : null}
          {engine?.lastError ? (
            <div className="mt-3 rounded-lg border border-[var(--err)]/25 bg-[var(--err)]/10 px-3 py-2 text-[11.5px] text-[var(--err)]">
              {engine.lastError}
            </div>
          ) : null}
        </div>

        <button
          type="button"
          onClick={() => void (running ? disconnect() : connect())}
          disabled={operationBusy}
          className={cn(
            "group relative flex h-[168px] w-[168px] shrink-0 items-center justify-center rounded-full transition-transform active:scale-[0.97]",
            running ? "animate-pulse-ring" : "",
          )}
          aria-label={running ? t("common.stop") : t("common.start")}
        >
          <span
            className={cn(
              "absolute inset-0 rounded-full border-[1.5px]",
              running ? "border-[var(--ok)]/30" : "border-line-strong",
            )}
          />
          <span
            className={cn(
              "absolute inset-[10px] rounded-full border",
              running ? "border-[var(--ok)]/20" : "border-line",
            )}
          />
          <span
            className={cn(
              "flex h-[122px] w-[122px] items-center justify-center rounded-full shadow-2xl transition-all duration-300",
              running
                ? "bg-gradient-to-br from-emerald-400/90 to-emerald-600/90 text-white shadow-emerald-500/25"
                : "bg-gradient-to-br from-[var(--brand)] to-[var(--brand-strong)] text-white shadow-blue-500/25",
              connecting && "opacity-80",
            )}
          >
            {connecting ? <RefreshCw size={34} className="animate-spin" /> : <Power size={36} strokeWidth={2} />}
          </span>
        </button>
      </div>
    </Card>
  );
}
