import {
  Activity,
  ArrowDownToLine,
  FileDown,
  BadgeInfo,
  CloudDownload,
  FolderOpen,
  Gauge,
  Info,
  Laptop,
  Link2,
  RefreshCw,
  RotateCcw,
  Save,
  ShieldCheck,
  Wrench,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";

import { PageHeader } from "@/components/layout/PageHeader";
import {
  Badge,
  Button,
  Card,
  CardHeader,
  Field,
  Modal,
  Row,
  Segmented,
  Select,
  Switch,
  TextInput,
} from "@/components/ui";
import { ROUTES } from "@/lib/constants";
import { formatDuration } from "@/lib/format";
import type { AppConfig, Language, ThemeMode } from "@/lib/types";
import { useAppStore } from "@/stores/app";

export function Settings() {
  const t = useAppStore((state) => state.t);
  const config = useAppStore((state) => state.config);
  const system = useAppStore((state) => state.system);
  const core = useAppStore((state) => state.core);
  const engine = useAppStore((state) => state.engine);
  const envelope = useAppStore((state) => state.envelope);
  const busy = useAppStore((state) => state.busy);
  const saveConfig = useAppStore((state) => state.saveConfig);
  const applyConfig = useAppStore((state) => state.applyConfig);
  const installHelper = useAppStore((state) => state.installHelper);
  const uninstallHelper = useAppStore((state) => state.uninstallHelper);
  const startHelperElevated = useAppStore((state) => state.startHelperElevated);
  const setSystemProxy = useAppStore((state) => state.setSystemProxy);
  const setAutostart = useAppStore((state) => state.setAutostart);
  const setThemeMode = useAppStore((state) => state.setThemeMode);
  const setLanguage = useAppStore((state) => state.setLanguage);
  const refreshSubscription = useAppStore((state) => state.refreshSubscription);
  const exportDiagnostics = useAppStore((state) => state.exportDiagnostics);
  const openPath = useAppStore((state) => state.openPath);
  const update = useAppStore((state) => state.update);
  const updateChecking = useAppStore((state) => state.updateChecking);
  const checkUpdate = useAppStore((state) => state.checkUpdate);
  const skipUpdate = useAppStore((state) => state.skipUpdate);
  const resetConfig = useAppStore((state) => state.resetConfig);
  const pushToast = useAppStore((state) => state.pushToast);
  const configRef = useRef<AppConfig | null>(null);

  const [draft, setDraft] = useState<AppConfig | null>(null);
  const [resetOpen, setResetOpen] = useState(false);

  useEffect(() => {
    if (!config) return;
    if (configRef.current === config) return;
    configRef.current = config;
    setDraft(structuredClone(config));
  }, [config]);

  if (!draft || !config) return null;

  const patch = (updater: (current: AppConfig) => AppConfig) => {
    setDraft((current) => (current ? updater(structuredClone(current)) : current));
  };

  const save = async (): Promise<boolean> => {
    if (!draft) return false;
    const ok = await saveConfig(draft);
    if (ok) {
      setThemeMode(draft.ui.theme);
      setLanguage(draft.ui.language);
    }
    return ok;
  };

  const saveAndApply = async () => {
    const ok = await save();
    if (!ok) return;
    if (engine?.running) {
      await applyConfig();
    }
  };

  return (
    <>
      <PageHeader
        eyebrow="PREFERENCES"
        title={t("settings.title")}
        subtitle={t("settings.subtitle")}
        actions={
          <>
            <Button size="sm" variant="secondary" icon={<Save size={13} />} loading={busy === "save"} onClick={() => void save()}>
              {t("common.save")}
            </Button>
            <Button size="sm" variant="primary" icon={<Activity size={13} />} loading={busy === "apply"} onClick={() => void saveAndApply()}>
              {t("common.saveAndApply")}
            </Button>
          </>
        }
      />

      <div className="grid grid-cols-1 gap-5 xl:grid-cols-2">
        <Card>
          <CardHeader title={t("settings.appearance")} icon={<Laptop size={15} />} />
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <span className="text-[12.5px] text-ink-soft">{t("settings.theme")}</span>
              <Segmented
                value={draft.ui.theme}
                onChange={(theme: ThemeMode) => {
                  setThemeMode(theme);
                  patch((current) => ({ ...current, ui: { ...current.ui, theme } }));
                }}
                options={[
                  { value: "system", label: t("settings.themeSystem") },
                  { value: "light", label: t("settings.themeLight") },
                  { value: "dark", label: t("settings.themeDark") },
                ]}
              />
            </div>
            <div className="flex items-center justify-between">
              <span className="text-[12.5px] text-ink-soft">{t("settings.language")}</span>
              <Segmented
                value={draft.ui.language}
                onChange={(language: Language) => {
                  setLanguage(language);
                  patch((current) => ({ ...current, ui: { ...current.ui, language } }));
                }}
                options={[
                  { value: "zh-CN", label: "简体中文" },
                  { value: "en-US", label: "English" },
                ]}
              />
            </div>
          </div>
        </Card>

        <Card>
          <CardHeader title={t("settings.behavior")} icon={<Gauge size={15} />} />
          <div className="space-y-4">
            <Switch
              checked={draft.ui.tray}
              onChange={(tray) => patch((current) => ({ ...current, ui: { ...current.ui, tray } }))}
              label={t("settings.tray")}
            />
            <Switch
              checked={draft.ui.autoLaunch}
              onChange={async (autoLaunch) => {
                patch((current) => ({ ...current, ui: { ...current.ui, autoLaunch } }));
                await setAutostart(autoLaunch);
              }}
              label={t("settings.autoLaunch")}
            />
            <Switch
              checked={draft.engine.connectOnLaunch}
              onChange={(connectOnLaunch) =>
                patch((current) => ({
                  ...current,
                  engine: { ...current.engine, connectOnLaunch },
                }))
              }
              label={t("settings.connectOnLaunch")}
            />
          </div>
        </Card>

        <Card className="xl:col-span-2">
          <CardHeader title={t("settings.engine")} icon={<Wrench size={15} />} />
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
            <Field label={t("settings.mixedPort")}>
              <TextInput
                type="number"
                value={draft.engine.mixedPort}
                onChange={(event) =>
                  patch((current) => ({
                    ...current,
                    engine: { ...current.engine, mixedPort: Number(event.target.value) },
                  }))
                }
              />
            </Field>
            <Field label={t("settings.controllerPort")}>
              <TextInput
                type="number"
                value={draft.engine.controllerPort}
                onChange={(event) =>
                  patch((current) => ({
                    ...current,
                    engine: { ...current.engine, controllerPort: Number(event.target.value) },
                  }))
                }
              />
            </Field>
            <Field label={t("settings.logLevel")}>
              <Select
                value={draft.engine.logLevel}
                onChange={(event) =>
                  patch((current) => ({
                    ...current,
                    engine: { ...current.engine, logLevel: event.target.value },
                  }))
                }
                options={["silent", "error", "warning", "info", "debug"].map((level) => ({
                  value: level,
                  label: level,
                }))}
              />
            </Field>
            <Field label={t("settings.tunStack")}>
              <Select
                value={draft.engine.tun.stack}
                onChange={(event) =>
                  patch((current) => ({
                    ...current,
                    engine: {
                      ...current.engine,
                      tun: { ...current.engine.tun, stack: event.target.value as AppConfig["engine"]["tun"]["stack"] },
                    },
                  }))
                }
                options={[
                  { value: "mixed", label: "mixed" },
                  { value: "system", label: "system" },
                  { value: "gvisor", label: "gvisor" },
                ]}
              />
            </Field>
            <Field label={t("settings.mtu")}>
              <TextInput
                type="number"
                value={draft.engine.tun.mtu}
                onChange={(event) =>
                  patch((current) => ({
                    ...current,
                    engine: {
                      ...current.engine,
                      tun: { ...current.engine.tun, mtu: Number(event.target.value) },
                    },
                  }))
                }
              />
            </Field>
            <div className="space-y-3 sm:col-span-2 lg:col-span-4">
              <div className="grid grid-cols-1 gap-x-8 gap-y-3 sm:grid-cols-2">
                <Switch
                  checked={draft.engine.tun.enabled}
                  onChange={(enabled) =>
                    patch((current) => ({
                      ...current,
                      engine: { ...current.engine, tun: { ...current.engine.tun, enabled } },
                    }))
                  }
                  label={t("settings.tunMode")}
                  description={t("mode.tunHint")}
                />
                <Switch
                  checked={draft.engine.tun.dnsHijack}
                  onChange={(dnsHijack) =>
                    patch((current) => ({
                      ...current,
                      engine: { ...current.engine, tun: { ...current.engine.tun, dnsHijack } },
                    }))
                  }
                  label={t("settings.dnsHijack")}
                />
                <Switch
                  checked={draft.engine.systemProxy}
                  onChange={(systemProxy) =>
                    patch((current) => ({ ...current, engine: { ...current.engine, systemProxy } }))
                  }
                  label={t("settings.systemProxy")}
                />
                <Switch
                  checked={draft.engine.allowLan}
                  onChange={(allowLan) =>
                    patch((current) => ({ ...current, engine: { ...current.engine, allowLan } }))
                  }
                  label={t("settings.allowLan")}
                />
                <Switch
                  checked={draft.engine.ipv6}
                  onChange={(ipv6) => patch((current) => ({ ...current, engine: { ...current.engine, ipv6 } }))}
                  label={t("settings.ipv6")}
                />
                <Switch
                  checked={draft.engine.closeOtherProxies}
                  onChange={(closeOtherProxies) =>
                    patch((current) => ({
                      ...current,
                      engine: { ...current.engine, closeOtherProxies },
                    }))
                  }
                  label={t("settings.closeOtherProxies")}
                  description={t("settings.closeOtherProxiesHint")}
                />
                <div className="flex items-center justify-between gap-3 rounded-xl border border-line bg-card px-3 py-2">
                  <span className="text-[11.5px] text-ink-muted">{t("status.systemProxyOn")}</span>
                  <div className="flex gap-1">
                    <Button size="sm" variant="secondary" onClick={() => void setSystemProxy(true)}>
                      开启
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => void setSystemProxy(false)}>
                      关闭
                    </Button>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </Card>

        <Card>
          <CardHeader
            title={t("settings.helper")}
            icon={<ShieldCheck size={15} />}
            actions={
              <Badge tone={system?.helperRunning ? "ok" : system?.helperInstalled ? "warn" : "muted"}>
                {system?.helperRunning
                  ? t("settings.helperRunning")
                  : system?.helperInstalled
                    ? t("common.configured")
                    : t("status.helperMissing")}
              </Badge>
            }
          />
          <p className="text-[11.5px] leading-5 text-ink-muted">{t("settings.helperDesc")}</p>
          <div className="mt-3 grid grid-cols-2 gap-2 text-[11px]">
            <div className="rounded-lg border border-line bg-card px-2.5 py-1.5">
              <div className="text-ink-muted">{t("settings.helperVersion")}</div>
              <div className="font-mono text-ink-soft">{system?.helperVersion ?? "—"}</div>
            </div>
            <div className="rounded-lg border border-line bg-card px-2.5 py-1.5">
              <div className="text-ink-muted">{t("settings.controllerPort")}</div>
              <div className="font-mono text-ink-soft">{engine?.controllerPort ?? "—"}</div>
            </div>
          </div>
          <div className="mt-3 flex flex-wrap gap-2">
            <Button size="sm" variant="primary" loading={busy === "helper"} onClick={() => void installHelper()}>
              {t("settings.helperInstall")}
            </Button>
            <Button size="sm" variant="secondary" onClick={() => void startHelperElevated()}>
              {t("settings.helperElevated")}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => void uninstallHelper()}>
              {t("settings.helperUninstall")}
            </Button>
          </div>
          {system?.portConflicts?.length ? (
            <div className="mt-3 space-y-1">
              {system.portConflicts.map((conflict, index) => (
                <div key={index} className="text-[10.5px] text-[var(--warn)]">
                  · {conflict}
                </div>
              ))}
            </div>
          ) : null}
        </Card>

        <Card>
          <CardHeader title={t("settings.resources")} icon={<CloudDownload size={15} />} />
          <div className="space-y-3">
            <Button
              size="sm"
              variant="secondary"
              icon={<RefreshCw size={13} />}
              loading={busy === "subscription"}
              onClick={() => void refreshSubscription()}
              block
            >
              {t("settings.refreshSubscription")}
            </Button>
            <Button size="sm" variant="secondary" icon={<FolderOpen size={13} />} onClick={() => void exportDiagnostics()} block>
              {t("settings.diagnostics")}
            </Button>
            <Button
              size="sm"
              variant="ghost"
              icon={<Link2 size={13} />}
              onClick={async () => {
                const json = await useAppStore.getState().exportConfig();
                if (json) {
                  try {
                    await navigator.clipboard.writeText(json);
                    pushToast("success", t("toast.copied"));
                  } catch {
                    pushToast("error", t("toast.failed"));
                  }
                }
              }}
              block
            >
              复制配置 JSON
            </Button>
            <Link to={ROUTES.merge} className="block text-[11.5px] text-brand hover:underline">
              {t("nav.merge")} →
            </Link>
          </div>
          <div className="mt-4 border-t border-line pt-4">
            <div className="text-[12px] font-medium text-ink">{t("settings.reset")}</div>
            <div className="hint mt-0.5">{t("settings.resetHint")}</div>
            <Button
              size="sm"
              variant="danger"
              className="mt-2"
              icon={<RotateCcw size={13} />}
              onClick={() => setResetOpen(true)}
            >
              {t("settings.reset")}
            </Button>
          </div>
        </Card>

        <Card>
          <CardHeader title={t("settings.paths")} icon={<FolderOpen size={15} />} />
          {[
            { label: t("settings.dataDir"), value: envelope?.paths.dataDir, target: "data" },
            { label: t("settings.configFile"), value: envelope?.paths.configFile, target: "data" },
            { label: t("settings.coreDir"), value: envelope?.paths.coreDir, target: "core" },
            { label: t("settings.logsDir"), value: envelope?.paths.logsDir, target: "logs" },
          ].map((item) => (
            <div key={item.label} className="flex items-center justify-between gap-3 border-b border-line py-2 last:border-0">
              <div className="min-w-0">
                <div className="text-[11.5px] text-ink-soft">{item.label}</div>
                <div className="truncate font-mono text-[10px] text-ink-muted" title={item.value}>
                  {item.value ?? "—"}
                </div>
              </div>
              <Button size="sm" variant="ghost" onClick={() => void openPath(item.target)}>
                {t("settings.openDir")}
              </Button>
            </div>
          ))}
          <Button size="sm" variant="secondary" icon={<FileDown size={12} />} onClick={() => void exportDiagnostics()} className="mt-3" block>
            {t("settings.diagnostics")}
          </Button>
        </Card>

        <Card>
          <CardHeader title={t("settings.about")} icon={<BadgeInfo size={15} />} />
          <Row label={t("settings.version")} value={`v${system?.appVersion ?? "0.1.0"}`} />
          <Row label={t("settings.coreVersion")} value={core?.version ?? t("common.unknown")} />
          <Row
            label={t("settings.latestVersion")}
            value={
              update
                ? `v${update.latest}${update.hasUpdate ? "" : `（${t("update.upToDate")}）`}`
                : t("update.notChecked")
            }
          />
          <Row label={t("settings.uptime")} value={formatDuration(system?.appUptimeSecs ?? 0)} />
          <Row label={t("settings.license")} value="MIT（见 NOTICE）" />
          <div className="mt-3 flex flex-wrap items-center gap-2">
            <Button
              size="sm"
              variant="secondary"
              icon={<RefreshCw size={12} />}
              loading={updateChecking}
              onClick={() => void checkUpdate(true)}
            >
              {t("update.check")}
            </Button>
            {update?.hasUpdate ? (
              <Button
                size="sm"
                variant="primary"
                icon={<ArrowDownToLine size={12} />}
                onClick={() => void openPath(update.releaseUrl)}
              >
                {t("update.download")}
              </Button>
            ) : null}
            {config?.update.skippedVersion ? (
              <Button size="sm" variant="ghost" onClick={() => void skipUpdate("")}>
                {t("update.resume", { version: config.update.skippedVersion })}
              </Button>
            ) : null}
          </div>
          {draft ? (
            <div className="mt-3">
              <Switch
                checked={draft.update.autoCheck}
                onChange={(autoCheck) =>
                  patch((current) => ({ ...current, update: { ...current.update, autoCheck } }))
                }
                label={t("update.autoCheck")}
                description={t("update.autoCheckHint")}
              />
            </div>
          ) : null}
          <div className="mt-3 flex items-start gap-2 rounded-lg border border-line bg-card px-2.5 py-2 text-[10.5px] text-ink-muted">
            <Info size={12} className="mt-0.5 shrink-0" />
            <span>
              LabVPN 为非官方开源项目。mihomo（GPL-3.0）与 fdu-connect（AGPL-3.0）已随 App 打包并以独立进程运行，许可证与源码归档见 Release。
            </span>
          </div>
          <div className="mt-2 flex items-start gap-2 rounded-lg border border-[var(--ok)]/20 bg-[var(--ok)]/5 px-2.5 py-2 text-[10.5px] text-ink-muted">
            <ShieldCheck size={12} className="mt-0.5 shrink-0 text-[var(--ok)]" />
            <span>所有账号密码/TOTP/私钥口令仅加密保存在本机，绝不上传云端；敏感临时配置文件在核心启动后即销毁。</span>
          </div>
          <div className="mt-2 flex items-center justify-between">
            <span className="font-mono text-[10px] text-ink-muted">github.com/echo-edai/LabVPN-client</span>
            <Link to={ROUTES.dashboard} className="text-[11px] text-brand hover:underline">
              返回总览
            </Link>
          </div>
        </Card>
      </div>

      <Modal
        open={resetOpen}
        title={t("settings.reset")}
        onClose={() => setResetOpen(false)}
        footer={
          <>
            <Button variant="ghost" onClick={() => setResetOpen(false)}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="danger"
              loading={busy === "reset"}
              onClick={async () => {
                const ok = await resetConfig();
                setResetOpen(false);
                if (ok) window.location.reload();
              }}
            >
              {t("common.confirm")}
            </Button>
          </>
        }
      >
        {t("settings.resetHint")} 此操作不可撤销。密钥库中的密码会保留，但需要重新填写配置向导。
      </Modal>
    </>
  );
}
