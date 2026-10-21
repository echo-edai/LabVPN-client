import {
  AlertTriangle,
  ArrowRight,
  CheckCircle2,
  CloudDownload,
  FileCode2,
  GraduationCap,
  Layers,
  Play,
  RefreshCw,
  Route,
  Save,
  SlidersHorizontal,
  TerminalSquare,
} from "lucide-react";
import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router-dom";

import { DiagnosticsPanel } from "@/components/features/DiagnosticsPanel";
import { WizardStepper } from "@/components/features/WizardStepper";
import { PageHeader } from "@/components/layout/PageHeader";
import {
  Badge,
  Button,
  Card,
  CardHeader,
  CodeBlock,
  Field,
  Segmented,
  Switch,
  Textarea,
  TextInput,
} from "@/components/ui";
import { ROUTES } from "@/lib/constants";
import { cn } from "@/lib/cn";
import type { RoutingPreset } from "@/lib/types";
import { useAppStore } from "@/stores/app";

export function Merge() {
  const t = useAppStore((state) => state.t);
  const config = useAppStore((state) => state.config);
  const presetList = useAppStore((state) => state.presets);
  const preview = useAppStore((state) => state.preview);
  const saveConfig = useAppStore((state) => state.saveConfig);
  const applyConfig = useAppStore((state) => state.applyConfig);
  const connect = useAppStore((state) => state.connect);
  const loadPreview = useAppStore((state) => state.loadPreview);
  const validateRules = useAppStore((state) => state.validateRules);
  const busy = useAppStore((state) => state.busy);
  const engine = useAppStore((state) => state.engine);
  const navigate = useNavigate();

  const [preset, setPreset] = useState<RoutingPreset>("smart");
  const [adblock, setAdblock] = useState(false);
  const [geoRules, setGeoRules] = useState(true);
  const [enhancedRules, setEnhancedRules] = useState(false);
  const [highPriority, setHighPriority] = useState(false);
  const [customRules, setCustomRules] = useState("");
  const [extraDomains, setExtraDomains] = useState("");
  const [previewTab, setPreviewTab] = useState<"rules" | "yaml">("rules");
  const [showPreview, setShowPreview] = useState(false);

  useEffect(() => {
    if (!config) return;
    setPreset(config.routing.preset);
    setAdblock(config.routing.adblock);
    setGeoRules(config.routing.geoRules);
    setEnhancedRules(config.routing.enhancedRules);
    setHighPriority(config.routing.customRulesHighPriority);
    setCustomRules(config.routing.customRules.join("\n"));
    setExtraDomains(config.routing.extraCampusDomains.join("\n"));
    void loadPreview();
  }, [config, loadPreview]);

  const buildDraft = () => {
    if (!config) return null;
    return {
      ...config,
      routing: {
        ...config.routing,
        preset,
        adblock,
        geoRules,
        enhancedRules,
        customRulesHighPriority: highPriority,
        customRules: customRules
          .split("\n")
          .map((line) => line.trim())
          .filter(Boolean),
        extraCampusDomains: extraDomains
          .split("\n")
          .map((line) => line.trim())
          .filter(Boolean),
      },
    };
  };

  const saveOnly = async (): Promise<boolean> => {
    const draft = buildDraft();
    if (!draft) return false;
    const ok = await saveConfig(draft);
    if (ok) await loadPreview();
    return ok;
  };

  const finish = async (andConnect: boolean) => {
    const draft = buildDraft();
    if (!draft) return;
    const ok = await saveConfig({ ...draft, ui: { ...draft.ui, setupCompleted: true, setupStep: 4 } });
    if (!ok) return;
    if (andConnect) {
      if (engine?.running) {
        await applyConfig();
      } else {
        await connect();
      }
    }
    navigate(ROUTES.dashboard);
  };

  if (!config) return null;

  const subscriptionConfigured = config.subscription.enabled && Boolean(config.subscription.url);
  const vpnConfigured = config.campusVpn.enabled && Boolean(config.campusVpn.username);
  const sshConfigured =
    config.sshProxy.enabled && config.sshProxy.routes.some((route) => route.value.trim());

  return (
    <>
      <WizardStepper current={4} />
      <PageHeader
        eyebrow={`${t("wizard.titleSuffix")} · STEP 4`}
        title={t("merge.title")}
        subtitle={t("merge.subtitle")}
        actions={
          <>
            <Button size="sm" variant="secondary" icon={<Save size={13} />} onClick={() => void saveOnly()}>
              {t("merge.apply")}
            </Button>
            <Button
              size="sm"
              variant="primary"
              icon={<Play size={13} />}
              onClick={() => void finish(true)}
              loading={busy === "save" || busy === "connect"}
            >
              {t("merge.finishSetup")}
            </Button>
          </>
        }
      />

      <div className="mb-5 grid grid-cols-1 gap-3 sm:grid-cols-3">
        {[
          {
            title: t("dashboard.subscriptionCard"),
            icon: CloudDownload,
            ok: subscriptionConfigured,
            detail: subscriptionConfigured ? `${config.subscription.proxyCount ?? 0} 个节点` : t("common.notConfigured"),
            to: ROUTES.setupSubscription,
          },
          {
            title: t("dashboard.vpnCard"),
            icon: GraduationCap,
            ok: vpnConfigured,
            detail: vpnConfigured ? `SOCKS :${config.campusVpn.socksPort}` : t("common.notConfigured"),
            to: ROUTES.setupCampus,
          },
          {
            title: t("dashboard.sshCard"),
            icon: TerminalSquare,
            ok: sshConfigured,
            detail: sshConfigured ? `${config.sshProxy.host}:${config.sshProxy.port}` : t("common.disabled"),
            to: ROUTES.setupSsh,
          },
        ].map((item) => (
          <Link key={item.title} to={item.to}>
            <Card interactive className="flex items-center gap-3 p-4">
              <div
                className={cn(
                  "flex h-9 w-9 items-center justify-center rounded-xl border",
                  item.ok
                    ? "border-[var(--ok)]/25 bg-[var(--ok)]/10 text-[var(--ok)]"
                    : "border-line bg-card text-ink-muted",
                )}
              >
                <item.icon size={16} />
              </div>
              <div className="min-w-0 flex-1">
                <div className="text-[12px] font-medium text-ink">{item.title}</div>
                <div className="truncate text-[10.5px] text-ink-muted">{item.detail}</div>
              </div>
              <Badge tone={item.ok ? "ok" : "muted"}>{item.ok ? t("common.configured") : t("common.notConfigured")}</Badge>
            </Card>
          </Link>
        ))}
      </div>

      <div className="grid grid-cols-1 gap-5 xl:grid-cols-3">
        <div className="space-y-5 xl:col-span-2">
          <Card>
            <CardHeader
              title={t("merge.preset")}
              subtitle="一套特调规则的骨架，可再叠加自定义规则"
              icon={<SlidersHorizontal size={15} />}
            />
            <div className="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3">
              {presetList.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => setPreset(item.id)}
                  className={cn(
                    "rounded-xl border p-3 text-left transition",
                    preset === item.id
                      ? "border-[var(--brand)]/50 bg-[var(--brand-soft)]"
                      : "border-line bg-card hover:border-line-strong",
                  )}
                >
                  <div className="flex items-center gap-2">
                    <span className="text-[12.5px] font-semibold text-ink">{item.label}</span>
                    {item.recommended ? (
                      <Badge tone="brand" className="!px-1.5 !py-0 !text-[9px]">
                        推荐
                      </Badge>
                    ) : null}
                  </div>
                  <div className="mt-1 line-clamp-2 text-[10.5px] leading-4 text-ink-muted">{item.description}</div>
                </button>
              ))}
            </div>
            <div className="mt-4 grid grid-cols-1 gap-4 sm:grid-cols-2">
              <Switch checked={adblock} onChange={setAdblock} label={t("merge.adblock")} />
              <Switch checked={geoRules} onChange={setGeoRules} label={t("merge.geoRules")} />
              <Switch checked={enhancedRules} onChange={setEnhancedRules} label={t("merge.enhancedRules")} />
              <Switch
                checked={highPriority}
                onChange={setHighPriority}
                label={t("merge.rulesHighPriority")}
                description={t("merge.rulesHighPriorityHint")}
              />
            </div>
            <div className="mt-4">
              <Field label={t("merge.extraDomains")} hint="每行一个域名，会同时写入 mihomo 校内规则与 RVPN 强制代理">
                <TextInput
                  value={extraDomains.replaceAll("\n", ", ")}
                  onChange={(event) => setExtraDomains(event.target.value.replaceAll(",", "\n"))}
                  className="font-mono text-[12px]"
                  placeholder="science.org, nature.com"
                />
              </Field>
            </div>
          </Card>

          <Card>
            <CardHeader
              title={t("merge.customRules")}
              subtitle={t("merge.customRulesHint")}
              icon={<FileCode2 size={15} />}
              actions={
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() =>
                    void validateRules(
                      customRules
                        .split("\n")
                        .map((line) => line.trim())
                        .filter(Boolean),
                    )
                  }
                >
                  {t("merge.validate")}
                </Button>
              }
            />
            <Textarea
              value={customRules}
              onChange={(event) => setCustomRules(event.target.value)}
              rows={6}
              placeholder={"DOMAIN-SUFFIX,openai.com,🚀 代理\nIP-CIDR,10.8.0.0/24,🐧 SSH 转发,no-resolve"}
            />
            <div className="mt-2 flex flex-wrap gap-1.5">
              {["DOMAIN", "DOMAIN-SUFFIX", "IP-CIDR", "GEOIP", "PROCESS-NAME", "RULE-SET"].map((type) => (
                <span key={type} className="chip font-mono text-[10px]">
                  {type}
                </span>
              ))}
            </div>
          </Card>
        </div>

        <div className="space-y-5">
          <Card>
            <CardHeader title={t("merge.summary")} icon={<Layers size={15} />} />
            <div className="grid grid-cols-2 gap-3">
              {[
                { label: t("merge.ruleCount"), value: preview?.stats.ruleCount ?? "—" },
                { label: t("merge.groupCount"), value: preview?.stats.groupCount ?? "—" },
                { label: t("merge.providerCount"), value: preview?.stats.providerCount ?? "—" },
                {
                  label: "TUN",
                  value: preview ? (preview.stats.tunEnabled ? t("common.enabled") : t("common.disabled")) : "—",
                },
              ].map((item) => (
                <div key={item.label} className="rounded-xl border border-line bg-card px-3 py-2.5">
                  <div className="text-[18px] font-semibold text-ink">{item.value}</div>
                  <div className="text-[10.5px] text-ink-muted">{item.label}</div>
                </div>
              ))}
            </div>
            <div className="mt-4">
              <div className="mb-1.5 flex items-center justify-between">
                <span className="text-[11px] font-medium text-ink-soft">{t("merge.warnings")}</span>
                <Button size="sm" variant="ghost" icon={<RefreshCw size={11} />} onClick={() => void loadPreview()}>
                  {t("common.refresh")}
                </Button>
              </div>
              {preview?.warnings.length ? (
                <div className="space-y-1">
                  {preview.warnings.map((warning, index) => (
                    <div key={index} className="flex items-start gap-1.5 text-[10.5px] leading-4 text-[var(--warn)]">
                      <AlertTriangle size={11} className="mt-0.5 shrink-0" />
                      <span>{warning}</span>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="flex items-center gap-1.5 text-[11px] text-[var(--ok)]">
                  <CheckCircle2 size={12} />
                  {t("merge.noWarnings")}
                </div>
              )}
            </div>
          </Card>

          <Card>
            <CardHeader
              title={t("merge.preview")}
              icon={<Route size={15} />}
              actions={
                <>
                  <Segmented
                    size="sm"
                    value={previewTab}
                    onChange={setPreviewTab}
                    options={[
                      { value: "rules", label: "规则" },
                      { value: "yaml", label: "YAML" },
                    ]}
                  />
                  <Button size="sm" variant="ghost" onClick={() => setShowPreview((value) => !value)}>
                    {showPreview ? "收起" : "展开"}
                  </Button>
                </>
              }
            />
            {showPreview ? (
              previewTab === "rules" ? (
                <div className="scroll-slim max-h-[320px] space-y-1 pr-1 font-mono text-[10.5px] leading-4 text-ink-soft">
                  {(preview?.rules ?? []).map((rule, index) => (
                    <div key={index} className="flex gap-2">
                      <span className="w-5 shrink-0 text-right text-ink-muted">{index + 1}</span>
                      <span className="break-all">{rule}</span>
                    </div>
                  ))}
                </div>
              ) : (
                <CodeBlock code={preview?.yaml ?? ""} maxHeight={320} />
              )
            ) : (
              <div className="text-[11.5px] leading-5 text-ink-muted">{t("merge.configPreviewHint")}</div>
            )}
          </Card>
        </div>
      </div>

      <div className="mt-5">
        <DiagnosticsPanel />
      </div>

      <div className="mt-6 flex flex-wrap items-center justify-between gap-3">
        <Button variant="ghost" onClick={() => navigate(ROUTES.setupSsh)}>
          {t("common.prev")}
        </Button>
        <div className="flex flex-wrap gap-2">
          <Button variant="secondary" icon={<Save size={14} />} loading={busy === "save"} onClick={() => void saveOnly()}>
            {t("merge.apply")}
          </Button>
          <Button
            variant="primary"
            icon={<ArrowRight size={14} />}
            loading={busy === "save" || busy === "connect"}
            onClick={() => void finish(true)}
          >
            {t("merge.applyAndConnect")}
          </Button>
        </div>
      </div>
    </>
  );
}
