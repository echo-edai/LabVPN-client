import { Activity, ExternalLink, Power, ScanLine, ShieldCheck } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { Badge, Button, Card, CardHeader, TextInput } from "@/components/ui";
import { api } from "@/lib/ipc";
import type { EnvironmentReport } from "@/lib/types";
import { useAppStore } from "@/stores/app";

export function EnvironmentPanel() {
  const t = useAppStore((state) => state.t);
  const busy = useAppStore((state) => state.busy);
  const running = useAppStore((state) => state.engine?.running);
  const refresh = useAppStore((state) => state.refresh);
  const disconnect = useAppStore((state) => state.disconnect);
  const [report, setReport] = useState<EnvironmentReport | null>(null);
  const [working, setWorking] = useState("");
  const [message, setMessage] = useState("");
  const [networkCheckedAt, setNetworkCheckedAt] = useState("");
  const [sshHost, setSshHost] = useState("");
  const [sshPort, setSshPort] = useState(22);
  const [sshResult, setSshResult] = useState("");
  const inFlight = useRef(false);
  const alive = useRef(true);

  const inspect = useCallback(async (network = false) => {
    if (inFlight.current) return;
    inFlight.current = true;
    setWorking(network ? "network" : "inspect");
    setMessage("");
    try {
      const result = await api.inspectEnvironment(network);
      if (alive.current && network) setNetworkCheckedAt(result.checkedAt);
      if (alive.current) setReport((previous) => (network ? result : { ...result, checks: previous?.checks ?? [] }));
    } catch (error) {
      if (alive.current) setMessage(String(error));
    } finally {
      inFlight.current = false;
      if (alive.current) setWorking("");
    }
  }, []);

  useEffect(() => {
    alive.current = true;
    void inspect();
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible" && !useAppStore.getState().busy) void inspect();
    }, 30000);
    return () => {
      alive.current = false;
      window.clearInterval(timer);
    };
  }, [inspect, running]);

  const action = async (name: string, run: () => Promise<unknown>) => {
    if (inFlight.current || busy) return;
    inFlight.current = true;
    setWorking(name);
    setMessage("");
    try {
      const outcome = await run();
      await refresh();
      const result = await api.inspectEnvironment(false);
      if (alive.current) {
        setReport(result);
        if (outcome && typeof outcome === "object" && "message" in outcome) setMessage(String(outcome.message));
      }
    } catch (error) {
      if (alive.current) setMessage(String(error));
    } finally {
      inFlight.current = false;
      if (alive.current) setWorking("");
    }
  };

  const disabled = Boolean(working || busy);
  const startupPending = report?.startupItems.some((item) => item.disabled !== true);
  return (
    <Card>
      <CardHeader title={t("environment.title")} subtitle={t("environment.subtitle")} icon={<Activity size={16} />} />
      <p className="mb-4 text-sm leading-6 text-ink-soft">{t("settings.cleanLaunchHint")}</p>
      <div className="flex flex-wrap gap-3">
        <Button
          disabled={disabled}
          loading={working === "network"}
          icon={<ScanLine size={16} />}
          onClick={() => void inspect(true)}
        >
          {t("environment.check")}
        </Button>
        <Button
          disabled={disabled || !report || (!report.coreRunning && !report.vpnPid && !report.proxyLeasePending)}
          loading={working === "disconnect"}
          icon={<Power size={16} />}
          onClick={() => void action("disconnect", disconnect)}
        >
          {t("environment.disconnect")}
        </Button>
        {report?.canDisableStartup ? (
          <Button
            disabled={disabled || !startupPending}
            loading={working === "startup"}
            icon={<ShieldCheck size={16} />}
            onClick={() => void action("startup", api.disableProxyStartup)}
          >
            {t("environment.disableStartup")}
          </Button>
        ) : null}
        <Button
          disabled={disabled}
          icon={<ExternalLink size={16} />}
          onClick={() => void action("settings", api.openStartupSettings)}
        >
          {t("environment.startupSettings")}
        </Button>
      </div>
      <p className="mt-3 text-xs leading-5 text-ink-muted">{t("environment.actionHint")}</p>
      <details className="mt-4 rounded-lg border border-line px-3 py-2">
        <summary className="cursor-pointer text-sm font-medium text-ink">{t("environment.sshTitle")}</summary>
        <div className="mt-3 flex flex-wrap items-end gap-3">
          <label className="min-w-48 flex-1 text-sm text-ink-soft">
            {t("environment.sshHost")}
            <TextInput
              className="mt-1"
              value={sshHost}
              placeholder="10.x.x.x"
              onChange={(event) => setSshHost(event.target.value)}
            />
          </label>
          <label className="w-24 text-sm text-ink-soft">
            {t("environment.sshPort")}
            <TextInput
              className="mt-1"
              type="number"
              min={1}
              max={65535}
              value={sshPort}
              onChange={(event) => setSshPort(Number(event.target.value))}
            />
          </label>
          <Button
            disabled={disabled || !sshHost.trim() || sshPort < 1 || sshPort > 65535}
            loading={working === "ssh"}
            onClick={() =>
              void action("ssh", async () => {
                const result = await api.testSsh(sshHost.trim(), sshPort);
                setSshResult(
                  (result.ok ? "OK" : t("environment.failed")) + " · " + result.elapsedMs + " ms · " + result.message,
                );
              })
            }
          >
            {t("environment.sshTest")}
          </Button>
        </div>
        <p className="mt-2 text-xs leading-5 text-ink-muted">{t("environment.sshHint")}</p>
        {sshResult ? (
          <p role="status" className="mt-2 break-words text-sm text-ink">
            {sshResult}
          </p>
        ) : null}
      </details>
      <div aria-live="polite" aria-busy={Boolean(working)}>
        {message ? (
          <p className="mt-3 break-words rounded-lg border border-line p-3 text-sm text-ink">{message}</p>
        ) : null}
        {report ? (
          <>
            <div className="mt-4 flex flex-wrap gap-2">
              <Badge tone={report.startupReady ? "ok" : "warn"}>
                {t(report.startupReady ? "environment.ready" : "environment.recovering")}
              </Badge>
              <Badge tone={report.coreAlive ? "ok" : report.coreRunning ? "err" : "muted"}>
                {t("environment.core")}:{" "}
                {report.coreAlive
                  ? "API OK"
                  : report.coreRunning
                    ? t("environment.noResponse")
                    : t("environment.stopped")}
                {report.corePid ? ` · PID ${report.corePid}` : ""}
              </Badge>
              <Badge tone={report.vpnPid ? "info" : "muted"}>
                VPN: {report.vpnState}
                {report.vpnPid ? ` · PID ${report.vpnPid}` : ""}
              </Badge>
              <Badge tone="muted">
                {t("environment.autostart")}: {t(report.autostart ? "environment.enabled" : "environment.disabled")}
              </Badge>
            </div>
            <p className="mt-3 text-xs text-ink-muted">
              {t("environment.checkedAt")}: {new Date(report.checkedAt).toLocaleString()}
            </p>
            {networkCheckedAt ? (
              <p className="mt-3 text-xs text-ink-muted">
                {t("environment.networkCheckedAt")}: {new Date(networkCheckedAt).toLocaleString()}
              </p>
            ) : null}
            {report.checks.length ? (
              <div className="mt-3 grid gap-3 md:grid-cols-2">
                {report.checks.map((check) => (
                  <div key={check.name} className="rounded-lg border border-line p-3 text-sm">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <span>
                        {t(
                          check.name === "systemRoute"
                            ? "environment.systemRoute"
                            : check.name === "campusDns"
                              ? "environment.campusDns"
                              : "environment.mixedProxy",
                        )}
                      </span>
                      <Badge tone={check.ok === null ? "muted" : check.ok ? "ok" : "err"}>
                        {check.ok === null
                          ? t("environment.skipped")
                          : `${check.ok ? "OK" : t("environment.failed")} · ${check.elapsedMs} ms`}
                      </Badge>
                    </div>
                    <p className="mt-2 break-words text-xs leading-5 text-ink-muted">{check.detail}</p>
                  </div>
                ))}
              </div>
            ) : (
              <p className="mt-3 text-xs leading-5 text-ink-muted">{t("environment.notTested")}</p>
            )}
            {report.warnings.map((warning) => (
              <p key={warning} className="mt-2 text-sm text-warn">
                {warning}
              </p>
            ))}
            <details className="mt-4 rounded-lg border border-line px-3 py-2">
              <summary className="cursor-pointer text-sm font-medium text-ink">{t("environment.details")}</summary>
              <div className="mt-3 space-y-3 text-sm leading-6 text-ink-soft">
                <div>
                  <p className="font-medium">{t("environment.processes")}</p>
                  <p className="break-words">{report.proxies.join(" / ") || t("environment.noneDetected")}</p>
                </div>
                <div>
                  <p className="font-medium">{t("environment.settings")}</p>
                  {report.proxySettings.map((line) => (
                    <p className="break-words font-mono text-xs" key={line}>
                      {line}
                    </p>
                  ))}
                </div>
                <div>
                  <p className="font-medium">{t("environment.startupItems")}</p>
                  {report.startupItems.map((item) => (
                    <p className="break-words font-mono text-xs" key={item.domain + item.label}>
                      {item.label}:{" "}
                      {t(
                        item.disabled === true
                          ? "environment.disabled"
                          : item.disabled === false
                            ? "environment.enabled"
                            : "environment.unknown",
                      )}
                    </p>
                  ))}
                  <p className="mt-1 text-xs text-ink-muted">{t("environment.coverage")}</p>
                </div>
                <p className="break-words">
                  {t("environment.variables")}: {report.environmentKeys.join(", ") || "—"}
                </p>
              </div>
            </details>
          </>
        ) : (
          <p className="mt-4 text-sm text-ink-muted">{t("environment.unknown")}</p>
        )}
      </div>
    </Card>
  );
}
