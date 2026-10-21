import {
  ArrowLeft,
  ArrowRight,
  CheckCircle2,
  GraduationCap,
  KeyRound,
  RadioTower,
  ScanSearch,
  ServerCog,
  ShieldCheck,
  XCircle,
} from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";

import { CampusCard } from "@/components/features/StatusCards";
import { ConfigTabs } from "@/components/features/ConfigTabs";
import { PageHeader } from "@/components/layout/PageHeader";
import { Badge, Button, Card, CardHeader, Field, PasswordInput, Row, Select, Switch, TextInput } from "@/components/ui";
import { ROUTES, SECRET_KEYS } from "@/lib/constants";
import type { ExistingVpn, ProbeTarget, VpnTestResult } from "@/lib/types";
import { useAppStore } from "@/stores/app";

interface DraftState {
  username: string;
  password: string;
  totp: string;
  server: string;
  port: number;
  serverIpOverride: string;
  socksPort: number;
  managed: boolean;
  binaryPath: string;
  autoDetectCampus: boolean;
  skipWhenOnCampus: boolean;
  extraDomains: string;
  probeTargets: ProbeTarget[];
  campusCidrs: string;
}

export function SetupCampusVpn() {
  const t = useAppStore((state) => state.t);
  const config = useAppStore((state) => state.config);
  const envelope = useAppStore((state) => state.envelope);
  const saveConfig = useAppStore((state) => state.saveConfig);
  const testVpn = useAppStore((state) => state.testVpn);
  const detectVpnBinary = useAppStore((state) => state.detectVpnBinary);
  const busy = useAppStore((state) => state.busy);
  const navigate = useNavigate();

  const [draft, setDraft] = useState<DraftState | null>(null);
  const [detected, setDetected] = useState<ExistingVpn | null>(null);
  const [testResult, setTestResult] = useState<VpnTestResult | null>(null);
  const [advanced, setAdvanced] = useState(false);

  useEffect(() => {
    if (!config) return;
    setDraft({
      username: config.campusVpn.username,
      password: "",
      totp: "",
      server: config.campusVpn.server,
      port: config.campusVpn.port,
      serverIpOverride: config.campusVpn.serverIpOverride ?? "",
      socksPort: config.campusVpn.socksPort,
      managed: config.campusVpn.managed,
      binaryPath: config.campusVpn.binaryPath ?? "",
      autoDetectCampus: config.campusVpn.autoDetectCampus,
      skipWhenOnCampus: config.campusVpn.skipWhenOnCampus,
      extraDomains: config.routing.extraCampusDomains.join("\n"),
      probeTargets: config.campusVpn.probeTargets,
      campusCidrs: config.campusVpn.campusCidrs.join("\n"),
    });
  }, [config]);

  const passwordSaved = envelope?.secretsPresent?.[SECRET_KEYS.campusPassword] ?? false;

  const persist = async (): Promise<boolean> => {
    if (!config || !draft) return false;
    const secrets: Record<string, string> = {};
    if (draft.password.trim()) secrets[SECRET_KEYS.campusPassword] = draft.password;
    if (draft.totp.trim()) secrets[SECRET_KEYS.campusTotp] = draft.totp;
    const ok = await saveConfig(
      {
        ...config,
        campusVpn: {
          ...config.campusVpn,
          enabled: true,
          username: draft.username.trim(),
          server: draft.server.trim(),
          port: draft.port,
          serverIpOverride: draft.serverIpOverride.trim() || null,
          socksPort: draft.socksPort,
          managed: draft.managed,
          binaryPath: draft.binaryPath.trim() || null,
          autoDetectCampus: draft.autoDetectCampus,
          skipWhenOnCampus: draft.skipWhenOnCampus,
          probeTargets: draft.probeTargets.filter((target) => target.host.trim()),
          campusCidrs: draft.campusCidrs
            .split("\n")
            .map((line) => line.trim())
            .filter(Boolean),
          customProxyDomains: draft.extraDomains
            .split("\n")
            .map((line) => line.trim())
            .filter(Boolean),
        },
        routing: {
          ...config.routing,
          extraCampusDomains: draft.extraDomains
            .split("\n")
            .map((line) => line.trim())
            .filter(Boolean),
        },
      },
      secrets,
    );
    if (ok) setDraft({ ...draft, password: "", totp: "" });
    return ok;
  };

  const updateTarget = (index: number, patch: Partial<ProbeTarget>) => {
    if (!draft) return;
    setDraft({
      ...draft,
      probeTargets: draft.probeTargets.map((target, targetIndex) =>
        targetIndex === index ? { ...target, ...patch } : target,
      ),
    });
  };

  if (!draft) return null;

  return (
    <>
      <ConfigTabs />
      <PageHeader
        eyebrow={t("configTabs.parallel")}
        title={t("setupCampus.title")}
        subtitle={t("setupCampus.subtitle")}
        actions={<Badge tone="info" icon={<GraduationCap size={11} />}>fdu-connect</Badge>}
      />

      <div className="grid grid-cols-1 gap-5 xl:grid-cols-3">
        <div className="space-y-5 xl:col-span-2">
          <Card>
            <CardHeader title={t("setupCampus.account")} icon={<KeyRound size={15} />} />
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <Field label={t("setupCampus.account")} required>
                <TextInput
                  value={draft.username}
                  onChange={(event) => setDraft({ ...draft, username: event.target.value })}
                  placeholder={t("setupCampus.accountPlaceholder")}
                  autoComplete="username"
                />
              </Field>
              <Field
                label={t("setupCampus.password")}
                required
                hint={
                  passwordSaved
                    ? t("setupCampus.passwordSaved")
                    : "密码仅经 AES-256-GCM 加密保存在本机，不会上传任何服务器"
                }
              >
                <PasswordInput
                  value={draft.password}
                  onChange={(event) => setDraft({ ...draft, password: event.target.value })}
                  placeholder={passwordSaved ? "••••••••（已保存）" : t("setupCampus.passwordPlaceholder")}
                />
              </Field>
              <Field label={t("setupCampus.totp")} hint={t("setupCampus.totpHint")}>
                <TextInput
                  value={draft.totp}
                  onChange={(event) => setDraft({ ...draft, totp: event.target.value })}
                  placeholder="JBSWY3DPEHPK3PXP"
                  className="font-mono text-[12px]"
                />
              </Field>
              <div className="flex items-end pb-2">
                <Badge tone={draft.autoDetectCampus ? "ok" : "muted"} icon={<ScanSearch size={11} />}>
                  {draft.autoDetectCampus ? t("setupCampus.campusDetect") : t("common.disabled")}
                </Badge>
              </div>
            </div>
          </Card>

          <Card>
            <CardHeader title={t("setupCampus.server")} icon={<ServerCog size={15} />} />
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
              <Field label={t("setupCampus.server")} className="sm:col-span-2">
                <TextInput
                  value={draft.server}
                  onChange={(event) => setDraft({ ...draft, server: event.target.value })}
                  className="font-mono text-[12px]"
                />
              </Field>
              <Field label={t("setupCampus.port")}>
                <TextInput
                  type="number"
                  value={draft.port}
                  onChange={(event) => setDraft({ ...draft, port: Number(event.target.value) })}
                />
              </Field>
              <Field label={t("setupCampus.socksPort")}>
                <TextInput
                  type="number"
                  value={draft.socksPort}
                  onChange={(event) => setDraft({ ...draft, socksPort: Number(event.target.value) })}
                />
              </Field>
              <Field label="服务器 IP（可选）" hint="校园网下建议填写以避开 fake-ip" className="sm:col-span-2">
                <TextInput
                  value={draft.serverIpOverride}
                  onChange={(event) => setDraft({ ...draft, serverIpOverride: event.target.value })}
                  placeholder="202.120.224.58"
                  className="font-mono text-[12px]"
                />
              </Field>
              <div className="space-y-3 sm:col-span-2">
                <Switch
                  checked={draft.managed}
                  onChange={(managed) => setDraft({ ...draft, managed })}
                  label={draft.managed ? t("setupCampus.managed") : t("setupCampus.reuse")}
                  description={draft.managed ? undefined : t("setupCampus.reuseHint")}
                />
              </div>
            </div>

            <div className="mt-4 grid grid-cols-1 gap-4 sm:grid-cols-2">
              <Field label={t("setupCampus.binary")} hint={t("setupCampus.detectHint")}>
                <div className="flex gap-2">
                  <TextInput
                    value={draft.binaryPath}
                    onChange={(event) => setDraft({ ...draft, binaryPath: event.target.value })}
                    placeholder={t("setupCampus.binaryPlaceholder")}
                    className="font-mono text-[12px]"
                  />
                  <Button
                    variant="secondary"
                    size="md"
                    icon={<ScanSearch size={14} />}
                    onClick={async () => {
                      const result = await detectVpnBinary();
                      setDetected(result);
                      if (result?.binaryPath) {
                        setDraft({ ...draft, binaryPath: result.binaryPath });
                      }
                    }}
                  >
                    {t("setupCampus.detect")}
                  </Button>
                </div>
              </Field>
              <Field label={t("setupCampus.extraDomains")} hint={t("setupCampus.extraDomainsHint")}>
                <TextInput
                  value={draft.extraDomains.replaceAll("\n", ", ")}
                  onChange={(event) =>
                    setDraft({ ...draft, extraDomains: event.target.value.replaceAll(",", "\n") })
                  }
                  placeholder="science.org, nature.com"
                  className="font-mono text-[12px]"
                />
              </Field>
            </div>
          </Card>

          <Card>
            <button
              type="button"
              className="flex w-full items-center justify-between text-left"
              onClick={() => setAdvanced((value) => !value)}
            >
              <span className="text-[13px] font-semibold text-ink">{t("setupCampus.advanced")}</span>
              <span className="text-[11px] text-ink-muted">{advanced ? "收起" : "展开"}</span>
            </button>
            {advanced ? (
              <div className="mt-4 space-y-4">
                <div className="space-y-2">
                  {draft.probeTargets.map((target, index) => (
                    <div key={index} className="grid grid-cols-[0.9fr_1.3fr_0.6fr_0.5fr_auto] items-center gap-2">
                      <Select
                        value={target.protocol}
                        onChange={(event) =>
                          updateTarget(index, { protocol: event.target.value as ProbeTarget["protocol"] })
                        }
                        options={[
                          { value: "tcp", label: "TCP" },
                          { value: "udpDns", label: "DNS" },
                          { value: "icmp", label: "Ping" },
                        ]}
                      />
                      <TextInput
                        value={target.host}
                        onChange={(event) => updateTarget(index, { host: event.target.value })}
                        className="font-mono text-[11.5px]"
                        placeholder="10.176.52.2"
                      />
                      <TextInput
                        type="number"
                        value={target.port}
                        disabled={target.protocol === "icmp"}
                        onChange={(event) => updateTarget(index, { port: Number(event.target.value) })}
                        title={target.protocol === "icmp" ? "Ping 无需端口" : "端口"}
                      />
                      <TextInput
                        type="number"
                        value={target.weight}
                        onChange={(event) => updateTarget(index, { weight: Number(event.target.value) })}
                        title="权重"
                      />
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() =>
                          setDraft({ ...draft, probeTargets: draft.probeTargets.filter((_, i) => i !== index) })
                        }
                      >
                        {t("common.remove")}
                      </Button>
                    </div>
                  ))}
                  <Button
                    size="sm"
                    variant="secondary"
                    onClick={() =>
                      setDraft({
                        ...draft,
                        probeTargets: [
                          ...draft.probeTargets,
                          { name: "自定义探测", host: "", port: 443, protocol: "tcp", weight: 60, timeoutMs: 1500 },
                        ],
                      })
                    }
                  >
                    {t("common.add")}
                  </Button>
                  <div className="rounded-lg border border-[var(--ok)]/20 bg-[var(--ok)]/5 px-2.5 py-1.5 text-[10.5px] leading-4 text-ink-muted">
                    Ping 探测：ping 通校内网关（默认 <span className="font-mono">10.134.140.1</span>）即
                    <span className="font-semibold text-[var(--ok)]"> 100% 判定在校内</span>，自动跳过 fdu-connect 并全部直连校内流量。
                  </div>
                </div>
                <Field label="校内 IP 段（每行一个）">
                  <TextInput
                    value={draft.campusCidrs.replaceAll("\n", ", ")}
                    onChange={(event) =>
                      setDraft({ ...draft, campusCidrs: event.target.value.replaceAll(",", "\n") })
                    }
                    className="font-mono text-[12px]"
                  />
                </Field>
                <div className="flex flex-wrap gap-4">
                  <Switch
                    checked={draft.autoDetectCampus}
                    onChange={(autoDetectCampus) => setDraft({ ...draft, autoDetectCampus })}
                    label={t("setupCampus.campusDetect")}
                  />
                  <Switch
                    checked={draft.skipWhenOnCampus}
                    onChange={(skipWhenOnCampus) => setDraft({ ...draft, skipWhenOnCampus })}
                    label="校内自动跳过 VPN"
                  />
                </div>
              </div>
            ) : null}
          </Card>

          <div className="flex flex-wrap gap-2">
            <Button
              variant="primary"
              loading={busy === "save"}
              onClick={async () => {
                const ok = await persist();
                if (ok) setTestResult(null);
              }}
            >
              {t("common.save")}
            </Button>
            <Button
              variant="secondary"
              loading={busy === "vpn-test"}
              onClick={async () => {
                const ok = await persist();
                if (ok) setTestResult(await testVpn());
              }}
              icon={<ShieldCheck size={14} />}
            >
              {t("setupCampus.test")}
            </Button>
          </div>
        </div>

        <div className="space-y-5">
          <CampusCard />
          <Card className="border-[var(--ok)]/20 bg-[var(--ok)]/5">
            <CardHeader title="凭据安全" icon={<ShieldCheck size={15} />} />
            <ul className="space-y-1.5 text-[11px] leading-4 text-ink-muted">
              <li>· 账号密码经 AES-256-GCM 加密存储在本机密钥库（0600），不写入配置、不上传云端。</li>
              <li>· fdu-connect 启动完成读取后，含密码的临时配置文件会立即被覆盖销毁。</li>
              <li>· 日志与诊断包自动脱敏，不包含密码。</li>
              <li>· App 内置 fdu-connect，无需额外安装即可使用。</li>
            </ul>
          </Card>
          <Card>
            <CardHeader title="fdu-connect 检测" icon={<RadioTower size={15} />} />
            {detected ? (
              <>
                <Row label="状态" value={detected.running ? "运行中" : "未运行"} />
                <Row label="SOCKS 端口" value={detected.socksPort ? String(detected.socksPort) : "—"} />
                <Row label="启动服务" value={detected.serviceLabel ?? "—"} />
                <div className="mt-2 break-all font-mono text-[10.5px] text-ink-muted">
                  {detected.binaryPath ?? t("setupCampus.notFound")}
                </div>
                {detected.binaryPath?.includes("Resources/bin") ? (
                  <Badge tone="ok" className="mt-2">
                    内置版本（随 App 安装）
                  </Badge>
                ) : null}
              </>
            ) : (
              <div className="text-[11.5px] leading-5 text-ink-muted">
                点击左侧「{t("setupCampus.detect")}」查找本机已安装的 fdu-connect（支持
                ~/LocalDocuments/fdu-connect、/usr/local/bin、PATH 等位置）。
              </div>
            )}
          </Card>
          {testResult ? (
            <Card>
              <CardHeader
                title={testResult.ok ? t("setupCampus.testOk") : t("setupCampus.testFailed")}
                icon={
                  testResult.ok ? (
                    <CheckCircle2 size={15} className="text-[var(--ok)]" />
                  ) : (
                    <XCircle size={15} className="text-[var(--err)]" />
                  )
                }
              />
              <div className="text-[11.5px] leading-5 text-ink-soft">{testResult.message}</div>
              <div className="mt-2 max-h-40 space-y-0.5 overflow-y-auto scroll-slim font-mono text-[10px] text-ink-muted">
                {testResult.logLines.map((line, index) => (
                  <div key={index}>{line}</div>
                ))}
              </div>
            </Card>
          ) : null}
        </div>
      </div>

      <div className="mt-6 flex items-center justify-between">
        <Button variant="ghost" icon={<ArrowLeft size={14} />} onClick={() => navigate(ROUTES.setupSubscription)}>
          {t("common.prev")}
        </Button>
        <div className="flex gap-2">
          <Button
            variant="secondary"
            onClick={async () => {
              const ok = await persist();
              if (ok) navigate(ROUTES.setupSsh);
            }}
          >
            {t("common.save")}
          </Button>
          <Button
            variant="primary"
            onClick={async () => {
              const ok = await persist();
              if (ok) navigate(ROUTES.setupSsh);
            }}
          >
            {t("common.next")}
            <ArrowRight size={14} />
          </Button>
        </div>
      </div>
    </>
  );
}
