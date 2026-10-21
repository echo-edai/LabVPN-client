import { ArrowLeft, CloudDownload, Link2, Radio, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";

import { ConfigTabs } from "@/components/features/ConfigTabs";
import { PageHeader } from "@/components/layout/PageHeader";
import { Badge, Button, Card, CardHeader, Field, Progress, Row, Switch, TextInput } from "@/components/ui";
import { ROUTES } from "@/lib/constants";
import { formatBytes, relativeTime, trafficUsage } from "@/lib/format";
import { useAppStore } from "@/stores/app";

export function SetupSubscription() {
  const t = useAppStore((state) => state.t);
  const config = useAppStore((state) => state.config);
  const subscription = useAppStore((state) => state.subscription);
  const saveConfig = useAppStore((state) => state.saveConfig);
  const importSubscription = useAppStore((state) => state.importSubscription);
  const testSubscription = useAppStore((state) => state.testSubscription);
  const busy = useAppStore((state) => state.busy);
  const navigate = useNavigate();

  const [url, setUrl] = useState("");
  const [name, setName] = useState("");
  const [autoUpdate, setAutoUpdate] = useState(12);
  const [remoteProvider, setRemoteProvider] = useState(true);
  const [subEnabled, setSubEnabled] = useState(true);
  const [usage, setUsage] = useState<ReturnType<typeof trafficUsage>>(null);

  useEffect(() => {
    if (!config) return;
    setUrl(config.subscription.url);
    setName(config.subscription.name);
    setAutoUpdate(config.subscription.updateIntervalHours);
    setRemoteProvider(config.subscription.useRemoteProvider);
    setSubEnabled(config.subscription.enabled);
    setUsage(trafficUsage(config.subscription.traffic));
  }, [config]);

  const persistSubscriptionPrefs = async () => {
    if (!config) return false;
    return saveConfig({
      ...config,
      subscription: {
        ...config.subscription,
        enabled: subEnabled && url.trim().length > 0,
        url: url.trim(),
        name: name.trim() || config.subscription.name,
        updateIntervalHours: autoUpdate,
        useRemoteProvider: remoteProvider,
      },
    });
  };

  return (
    <>
      <ConfigTabs />
      <PageHeader
        eyebrow={t("configTabs.parallel")}
        title={t("setupSubscription.title")}
        subtitle={t("setupSubscription.subtitle")}
        actions={<Badge tone="brand" icon={<CloudDownload size={11} />}>Clash / mihomo</Badge>}
      />

      <div className="grid grid-cols-1 gap-5 xl:grid-cols-3">
        <Card className="xl:col-span-2">
          <CardHeader title={t("setupSubscription.urlLabel")} icon={<Link2 size={15} />} />
          <div className="space-y-4">
            <Field label={t("setupSubscription.urlLabel")} hint={t("setupSubscription.initHint")}>
              <TextInput
                value={url}
                onChange={(event) => setUrl(event.target.value)}
                placeholder={t("setupSubscription.urlPlaceholder")}
                className="font-mono text-[12px]"
                spellCheck={false}
              />
            </Field>
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
              <Field label={t("setupSubscription.nameLabel")} className="sm:col-span-1">
                <TextInput value={name} onChange={(event) => setName(event.target.value)} placeholder={t("setupSubscription.namePlaceholder")} />
              </Field>
              <Field label={t("setupSubscription.autoUpdate")}>
                <TextInput
                  type="number"
                  min={0}
                  max={168}
                  value={autoUpdate}
                  onChange={(event) => setAutoUpdate(Number(event.target.value))}
                />
              </Field>
              <div className="flex items-end pb-2">
                <Switch checked={subEnabled} onChange={setSubEnabled} label={t("common.enabled")} />
              </div>
            </div>
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
              <div className="flex items-end pb-2 sm:col-span-1">
                <Switch
                  checked={remoteProvider}
                  onChange={setRemoteProvider}
                  label={t("setupSubscription.remoteProvider")}
                />
              </div>
              <div className="sm:col-span-2" />
            </div>
            <div className="hint">{t("setupSubscription.remoteProviderHint")}</div>

            <div className="flex flex-wrap gap-2 pt-1">
              <Button
                variant="primary"
                icon={<CloudDownload size={15} />}
                loading={busy === "subscription"}
                onClick={async () => {
                  const ok = await importSubscription(url.trim(), name.trim() || undefined);
                  if (ok) await persistSubscriptionPrefs();
                }}
                disabled={!url.trim()}
              >
                {t("setupSubscription.import")}
              </Button>
              <Button
                variant="secondary"
                icon={<Radio size={14} />}
                loading={busy === "subscription-test"}
                onClick={() => void testSubscription(url.trim())}
                disabled={!url.trim()}
              >
                {t("setupSubscription.test")}
              </Button>
              <Button
                variant="ghost"
                icon={<RefreshCw size={13} />}
                onClick={() => void persistSubscriptionPrefs()}
              >
                {t("common.save")}
              </Button>
            </div>
          </div>
        </Card>

        <div className="space-y-5">
          <Card>
            <CardHeader
              title={t("setupSubscription.current")}
              icon={<CloudDownload size={15} />}
              actions={
                <Badge tone={subscription?.enabled ? "ok" : "muted"}>
                  {subscription?.enabled ? t("common.enabled") : t("common.disabled")}
                </Badge>
              }
            />
            <Row label={t("setupSubscription.format")} value={subscription?.format ?? "—"} />
            <Row label={t("setupSubscription.nodeCount")} value={`${subscription?.proxyCount ?? 0}`} />
            <Row label={t("setupSubscription.groupCount")} value={`${subscription?.groupCount ?? 0}`} />
            <Row label={t("setupSubscription.lastUpdate")} value={relativeTime(subscription?.lastUpdated ?? null)} />
            {usage ? (
              <div className="mt-3">
                <div className="mb-1.5 flex items-center justify-between text-[11px]">
                  <span className="text-ink-muted">{t("setupSubscription.usage")}</span>
                  <span className="text-ink-soft">
                    {formatBytes(usage.used)} / {usage.total > 0 ? formatBytes(usage.total) : "∞"}
                  </span>
                </div>
                <Progress ratio={usage.ratio} tone={usage.ratio > 0.9 ? "err" : "brand"} />
                <div className="mt-1.5 text-[10.5px] text-ink-muted">{usage.expireText}</div>
              </div>
            ) : null}
            {subscription?.warnings?.length ? (
              <div className="mt-3 space-y-1">
                {subscription.warnings.map((warning, index) => (
                  <div key={index} className="text-[10.5px] text-[var(--warn)]">
                    · {warning}
                  </div>
                ))}
              </div>
            ) : null}
          </Card>

          <Card className="bg-[var(--brand-soft)]/40">
            <div className="text-[12px] font-semibold text-ink">提示</div>
            <ul className="mt-2 space-y-1.5 text-[11.5px] leading-4 text-ink-muted">
              <li>· 机场后台通常提供「Clash 订阅」，直接粘贴即可。</li>
              <li>· 支持 http(s) 与本地 file:// 配置。</li>
              <li>· 订阅 token 会脱敏保存在本机配置中。</li>
            </ul>
          </Card>
        </div>
      </div>

      <div className="mt-6 flex items-center justify-between">
        <Button variant="ghost" icon={<ArrowLeft size={14} />} onClick={() => navigate(ROUTES.dashboard)}>
          返回总览
        </Button>
        <Button
          variant="primary"
          onClick={async () => {
            await persistSubscriptionPrefs();
            navigate(ROUTES.dashboard);
          }}
        >
          {t("common.save")}
        </Button>
      </div>
    </>
  );
}
