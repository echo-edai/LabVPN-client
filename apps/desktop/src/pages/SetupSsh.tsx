import {
  ArrowLeft,
  ArrowRight,
  CheckCircle2,
  KeyRound,
  Network,
  Plus,
  Server,
  ShieldAlert,
  TerminalSquare,
  XCircle,
} from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";

import { WizardStepper } from "@/components/features/WizardStepper";
import { PageHeader } from "@/components/layout/PageHeader";
import {
  Badge,
  Button,
  Card,
  CardHeader,
  Field,
  PasswordInput,
  Segmented,
  Switch,
  TextInput,
} from "@/components/ui";
import { ROUTES, SECRET_KEYS } from "@/lib/constants";
import type { SshAuthType, SshTestResult } from "@/lib/types";
import { useAppStore } from "@/stores/app";

interface DraftState {
  enabled: boolean;
  host: string;
  port: number;
  username: string;
  auth: SshAuthType;
  password: string;
  keyPath: string;
  passphrase: string;
  routes: string[];
}

export function SetupSsh() {
  const t = useAppStore((state) => state.t);
  const config = useAppStore((state) => state.config);
  const envelope = useAppStore((state) => state.envelope);
  const saveConfig = useAppStore((state) => state.saveConfig);
  const testSsh = useAppStore((state) => state.testSsh);
  const busy = useAppStore((state) => state.busy);
  const navigate = useNavigate();

  const [draft, setDraft] = useState<DraftState | null>(null);
  const [testResult, setTestResult] = useState<SshTestResult | null>(null);

  useEffect(() => {
    if (!config) return;
    setDraft({
      enabled: config.sshProxy.enabled,
      host: config.sshProxy.host,
      port: config.sshProxy.port,
      username: config.sshProxy.username,
      auth: config.sshProxy.auth,
      password: "",
      keyPath: config.sshProxy.privateKeyPath ?? "",
      passphrase: "",
      routes: config.sshProxy.routes.map((route) => route.value),
    });
  }, [config]);

  const passwordSaved = envelope?.secretsPresent?.[SECRET_KEYS.sshPassword] ?? false;

  const persist = async (): Promise<boolean> => {
    if (!config || !draft) return false;
    const secrets: Record<string, string> = {};
    if (draft.auth === "password" && draft.password.trim()) {
      secrets[SECRET_KEYS.sshPassword] = draft.password;
    }
    if (draft.auth === "privateKey" && draft.passphrase.trim()) {
      secrets[SECRET_KEYS.sshKeyPassphrase] = draft.passphrase;
    }
    const ok = await saveConfig(
      {
        ...config,
        sshProxy: {
          ...config.sshProxy,
          enabled: draft.enabled,
          host: draft.host.trim(),
          port: draft.port,
          username: draft.username.trim(),
          auth: draft.auth,
          privateKeyPath: draft.auth === "privateKey" ? draft.keyPath.trim() || null : null,
          passwordRef: draft.auth === "password" ? SECRET_KEYS.sshPassword : null,
          privateKeyPassphraseRef:
            draft.auth === "privateKey" ? SECRET_KEYS.sshKeyPassphrase : null,
          routes: draft.routes
            .map((value) => value.trim())
            .filter(Boolean)
            .map((value) => ({ value })),
        },
      },
      secrets,
    );
    if (ok) setDraft({ ...draft, password: "", passphrase: "" });
    return ok;
  };

  if (!draft) return null;

  return (
    <>
      <WizardStepper current={3} />
      <PageHeader
        eyebrow={`${t("wizard.titleSuffix")} · STEP 3`}
        title={t("setupSsh.title")}
        subtitle={t("setupSsh.subtitle")}
        actions={<Badge tone="warn" icon={<TerminalSquare size={11} />}>ssh outbound</Badge>}
      />

      <div className="grid grid-cols-1 gap-5 xl:grid-cols-3">
        <div className="space-y-5 xl:col-span-2">
          <Card>
            <CardHeader
              title={t("setupSsh.enable")}
              icon={<Server size={15} />}
              actions={
                <Switch
                  checked={draft.enabled}
                  onChange={(enabled) => setDraft({ ...draft, enabled })}
                />
              }
            />
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
              <Field label={t("setupSsh.host")} className="sm:col-span-2">
                <TextInput
                  value={draft.host}
                  onChange={(event) => setDraft({ ...draft, host: event.target.value })}
                  placeholder={t("setupSsh.hostPlaceholder")}
                  className="font-mono text-[12px]"
                  disabled={!draft.enabled}
                />
              </Field>
              <Field label={t("setupSsh.port")}>
                <TextInput
                  type="number"
                  value={draft.port}
                  onChange={(event) => setDraft({ ...draft, port: Number(event.target.value) })}
                  disabled={!draft.enabled}
                />
              </Field>
              <Field label={t("setupSsh.username")}>
                <TextInput
                  value={draft.username}
                  onChange={(event) => setDraft({ ...draft, username: event.target.value })}
                  className="font-mono text-[12px]"
                  disabled={!draft.enabled}
                />
              </Field>
            </div>

            <div className="mt-4 flex flex-wrap items-center gap-3">
              <span className="label !mb-0">{t("setupSsh.authType")}</span>
              <Segmented
                value={draft.auth}
                onChange={(auth) => setDraft({ ...draft, auth })}
                options={[
                  { value: "password", label: t("setupSsh.password") },
                  { value: "privateKey", label: t("setupSsh.privateKey") },
                ]}
              />
            </div>

            <div className="mt-4 grid grid-cols-1 gap-4 sm:grid-cols-2">
              {draft.auth === "password" ? (
                <Field
                  label={t("setupSsh.password")}
                  hint={passwordSaved ? t("setupSsh.passwordSaved") : undefined}
                >
                  <PasswordInput
                    value={draft.password}
                    onChange={(event) => setDraft({ ...draft, password: event.target.value })}
                    placeholder={passwordSaved ? "••••••••（已保存）" : "SSH password"}
                    disabled={!draft.enabled}
                  />
                </Field>
              ) : (
                <>
                  <Field label={t("setupSsh.privateKey")} hint={t("setupSsh.privateKeyPlaceholder")}>
                    <TextInput
                      value={draft.keyPath}
                      onChange={(event) => setDraft({ ...draft, keyPath: event.target.value })}
                      placeholder="~/.ssh/id_ed25519"
                      className="font-mono text-[12px]"
                      disabled={!draft.enabled}
                    />
                  </Field>
                  <Field label={t("setupSsh.passphrase")}>
                    <PasswordInput
                      value={draft.passphrase}
                      onChange={(event) => setDraft({ ...draft, passphrase: event.target.value })}
                      placeholder="Passphrase"
                      disabled={!draft.enabled}
                    />
                  </Field>
                </>
              )}
            </div>
          </Card>

          <Card>
            <CardHeader
              title={t("setupSsh.routes")}
              subtitle="mihomo ssh outbound：匹配的域名 / CIDR 流量将经 SSH 服务器转发"
              icon={<Network size={15} />}
              actions={
                <Button
                  size="sm"
                  variant="secondary"
                  icon={<Plus size={12} />}
                  onClick={() => setDraft({ ...draft, routes: [...draft.routes, ""] })}
                  disabled={!draft.enabled}
                >
                  {t("setupSsh.addRoute")}
                </Button>
              }
            />
            <div className="space-y-2">
              {draft.routes.length === 0 ? (
                <div className="hint">{t("setupSsh.routePlaceholder")}</div>
              ) : null}
              {draft.routes.map((route, index) => (
                <div key={index} className="flex items-center gap-2">
                  <TextInput
                    value={route}
                    onChange={(event) =>
                      setDraft({
                        ...draft,
                        routes: draft.routes.map((value, routeIndex) =>
                          routeIndex === index ? event.target.value : value,
                        ),
                      })
                    }
                    placeholder={t("setupSsh.routePlaceholder")}
                    className="font-mono text-[12px]"
                    disabled={!draft.enabled}
                  />
                  <Button
                    size="sm"
                    variant="ghost"
                    className="shrink-0 whitespace-nowrap"
                    onClick={() => setDraft({ ...draft, routes: draft.routes.filter((_, i) => i !== index) })}
                  >
                    {t("common.remove")}
                  </Button>
                </div>
              ))}
            </div>
          </Card>

          <div className="flex flex-wrap gap-2">
            <Button variant="primary" loading={busy === "save"} onClick={() => void persist()}>
              {t("setupSsh.save")}
            </Button>
            <Button
              variant="secondary"
              icon={<KeyRound size={14} />}
              loading={busy === "ssh-test"}
              onClick={async () => {
                const ok = await persist();
                if (!ok) return;
                const result = await testSsh(draft.host.trim(), draft.port);
                setTestResult(result);
              }}
              disabled={!draft.host.trim()}
            >
              测试连通性
            </Button>
          </div>
        </div>

        <div className="space-y-5">
          {testResult ? (
            <Card>
              <CardHeader
                title={testResult.ok ? t("toast.testOk") : t("toast.testFailed")}
                icon={
                  testResult.ok ? (
                    <CheckCircle2 size={15} className="text-[var(--ok)]" />
                  ) : (
                    <XCircle size={15} className="text-[var(--err)]" />
                  )
                }
              />
              <div className="text-[11.5px] text-ink-soft">{testResult.message}</div>
              <div className="mt-1 text-[10.5px] text-ink-muted">{testResult.elapsedMs} ms</div>
            </Card>
          ) : null}

          <Card className="bg-[var(--warn)]/5">
            <CardHeader title="工作原理" icon={<ShieldAlert size={15} />} />
            <ul className="space-y-2 text-[11.5px] leading-5 text-ink-muted">
              <li>· LabVPN 使用 mihomo 原生 `ssh` 出站：在服务器上建立 SSH 连接，匹配规则的 TCP 流量经其转发。</li>
              <li>· SSH 只转发 TCP，不处理 UDP；DNS 仍由分流规则决定。</li>
              <li>· 密码/私钥口令保存在本机加密密钥库，写入 mihomo 配置时文件权限为 0600。</li>
              <li>· 这是「出口选择」，不会在服务器上留下全局代理规则。</li>
            </ul>
          </Card>

          <Card>
            <div className="text-[12.5px] font-semibold text-ink">{t("setupSsh.skipHint")}</div>
            <div className="mt-2 flex gap-2">
              <Button variant="secondary" size="sm" onClick={() => navigate(ROUTES.merge)}>
                {t("common.skip")}
              </Button>
            </div>
          </Card>
        </div>
      </div>

      <div className="mt-6 flex items-center justify-between">
        <Button variant="ghost" icon={<ArrowLeft size={14} />} onClick={() => navigate(ROUTES.setupCampus)}>
          {t("common.prev")}
        </Button>
        <Button
          variant="primary"
          onClick={async () => {
            const ok = await persist();
            if (ok) navigate(ROUTES.merge);
          }}
        >
          {t("common.next")}
          <ArrowRight size={14} />
        </Button>
      </div>
    </>
  );
}
