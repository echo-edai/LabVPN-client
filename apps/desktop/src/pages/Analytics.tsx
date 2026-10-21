import { Activity, Cpu, Globe2, GraduationCap, Plus, RefreshCw, Save, ShieldCheck, TerminalSquare, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { PageHeader } from "@/components/layout/PageHeader";
import { Badge, Button, Card, CardHeader, EmptyState, Field, Progress, Segmented, TextInput } from "@/components/ui";
import { formatBytes, relativeTime } from "@/lib/format";
import { useAppStore } from "@/stores/app";

type Range = "24h" | "7d" | "30d";

const CATEGORY_COLORS: Record<string, string> = {
  school: "var(--ok)",
  overseas: "var(--brand)",
  direct: "var(--accent)",
  ssh: "var(--warn)",
  other: "var(--ink-muted)",
};

export function Analytics() {
  const t = useAppStore((state) => state.t);
  const analytics = useAppStore((state) => state.analytics);
  const range = useAppStore((state) => state.analyticsRange);
  const loadAnalytics = useAppStore((state) => state.loadAnalytics);
  const updateAiDomains = useAppStore((state) => state.updateAiDomains);
  const engine = useAppStore((state) => state.engine);
  const [aiInput, setAiInput] = useState("");
  const [aiSaving, setAiSaving] = useState(false);

  useEffect(() => {
    void loadAnalytics();
  }, [loadAnalytics]);

  const categories = [
    { key: "school", label: t("analytics.school") },
    { key: "overseas", label: t("analytics.overseas") },
    { key: "direct", label: t("analytics.direct") },
    { key: "ssh", label: t("analytics.ssh") },
    { key: "other", label: t("analytics.other") },
  ] as const;

  const totals = analytics?.totals;
  const buckets = analytics?.buckets ?? [];
  const bucketTotal = (bucket: (typeof buckets)[number]) =>
    categories.reduce((sum, category) => sum + bucket[category.key].up + bucket[category.key].down, 0);
  const maxValue = Math.max(1, ...buckets.map(bucketTotal));

  const chart = useMemo(() => {
    if (buckets.length === 0) return null;
    const width = 900;
    const height = 150;
    return buckets.map((bucket, index) => {
      const x = (width / buckets.length) * index + 1;
      let cursor = height;
      const segments = categories.map((category) => {
        const value = bucket[category.key].up + bucket[category.key].down;
        const barHeight = maxValue === 0 ? 0 : (value / maxValue) * (height - 12);
        const y = cursor - barHeight;
        cursor = y;
        return { key: category.key, y, height: barHeight };
      });
      return { x, segments };
    });
  }, [buckets, maxValue, categories]);

  const egress = analytics?.egress ?? [];
  const proxyIps = egress.filter((item) => item.route === "proxy");
  const directIps = egress.filter((item) => item.route === "direct");
  const aiDomains = analytics?.aiDomains ?? [];

  const addAiDomain = async () => {
    const value = aiInput.trim().toLowerCase();
    if (!value) return;
    setAiSaving(true);
    const ok = await updateAiDomains([...aiDomains, value]);
    setAiSaving(false);
    if (ok) setAiInput("");
  };

  const removeAiDomain = async (domain: string) => {
    setAiSaving(true);
    await updateAiDomains(aiDomains.filter((item) => item !== domain));
    setAiSaving(false);
  };

  return (
    <>
      <PageHeader
        eyebrow={t("analytics.eyebrow")}
        title={t("analytics.title")}
        subtitle={t("analytics.subtitle")}
        actions={
          <div className="flex items-center gap-2">
            <Badge tone={engine?.running ? "ok" : "muted"} icon={<Activity size={11} />}>
              {engine?.running ? t("analytics.sampling") : t("status.disconnected")}
            </Badge>
            <Button size="sm" variant="secondary" icon={<RefreshCw size={12} />} onClick={() => void loadAnalytics()}>
              {t("common.refresh")}
            </Button>
          </div>
        }
      />

      <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <Segmented
          value={range}
          onChange={(value) => void loadAnalytics(value as Range)}
          options={[
            { value: "24h", label: t("analytics.range24h") },
            { value: "7d", label: t("analytics.range7d") },
            { value: "30d", label: t("analytics.range30d") },
          ]}
        />
        <div className="text-[10.5px] text-ink-muted">
          {t("analytics.localOnly")}
          {analytics ? ` · ${t("analytics.updatedAt")} ${relativeTime(analytics.updatedAt)}` : ""}
        </div>
      </div>

      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {[
          { key: "school", label: t("analytics.school"), icon: GraduationCap, color: "var(--ok)" },
          { key: "overseas", label: t("analytics.overseas"), icon: Globe2, color: "var(--brand)" },
          { key: "direct", label: t("analytics.direct"), icon: Cpu, color: "var(--accent)" },
          { key: "ssh", label: t("analytics.ssh"), icon: TerminalSquare, color: "var(--warn)" },
        ].map((item) => {
          const value = totals ? totals[item.key as keyof typeof totals] : { up: 0, down: 0 };
          const total = value.up + value.down;
          const grand = totals
            ? Object.values(totals).reduce((sum, category) => sum + category.up + category.down, 0)
            : 0;
          return (
            <Card key={item.key} className="p-3.5">
              <div className="flex items-center gap-2 text-[11px] text-ink-muted">
                <item.icon size={13} style={{ color: item.color }} />
                {item.label}
              </div>
              <div className="mt-1.5 text-[20px] font-semibold tracking-tight text-ink">{formatBytes(total)}</div>
              <div className="mt-2">
                <Progress ratio={grand > 0 ? total / grand : 0} tone="brand" />
              </div>
            </Card>
          );
        })}
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-3">
        <Card className="xl:col-span-2">
          <CardHeader title={t("analytics.trafficTrend")} subtitle={t("analytics.trafficHint")} icon={<Activity size={15} />} />
          {chart && chart.length > 0 ? (
            <>
              <svg viewBox={`0 0 900 160`} className="h-[170px] w-full" preserveAspectRatio="none">
                {[0.25, 0.5, 0.75].map((ratio) => (
                  <line key={ratio} x1="0" x2="900" y1={150 * ratio} y2={150 * ratio} stroke="var(--line)" strokeDasharray="3 5" />
                ))}
                {chart.map((bar, index) => (
                  <g key={index}>
                    {bar.segments.map((segment) => (
                      <rect
                        key={segment.key}
                        x={bar.x}
                        y={segment.y}
                        width={Math.max(1.5, 900 / chart.length - 3)}
                        height={segment.height}
                        fill={CATEGORY_COLORS[segment.key]}
                        opacity={0.85}
                        rx={1}
                      />
                    ))}
                  </g>
                ))}
              </svg>
              <div className="mt-2 flex flex-wrap gap-3 text-[10.5px] text-ink-muted">
                {categories.map((category) => (
                  <span key={category.key} className="inline-flex items-center gap-1">
                    <span className="h-1.5 w-1.5 rounded-full" style={{ background: CATEGORY_COLORS[category.key] }} />
                    {category.label}
                  </span>
                ))}
                <span className="ml-auto">
                  {analytics?.granularity === "hour" ? t("analytics.perHour") : t("analytics.perDay")}
                </span>
              </div>
            </>
          ) : (
            <EmptyState icon={<Activity size={22} />} title={t("analytics.noData")} hint={t("analytics.noDataHint")} />
          )}
        </Card>

        <Card>
          <CardHeader title={t("analytics.egressTitle")} subtitle={t("analytics.egressHint")} icon={<ShieldCheck size={15} />} />
          <div className="space-y-3">
            <div>
              <div className="text-[11px] font-medium text-ink-soft">{t("analytics.directIp")}</div>
              <div className="mt-1 space-y-1">
                {directIps.slice(-4).reverse().map((item) => (
                  <div key={`${item.ts}-${item.ip}`} className="flex items-center justify-between text-[11px]">
                    <span className="font-mono text-ink">{item.ip}</span>
                    <span className="text-ink-muted">{relativeTime(new Date(item.ts * 1000).toISOString())}</span>
                  </div>
                ))}
                {directIps.length === 0 ? <div className="text-[10.5px] text-ink-muted">—</div> : null}
              </div>
            </div>
            <div>
              <div className="text-[11px] font-medium text-ink-soft">{t("analytics.proxyIp")}</div>
              <div className="mt-1 space-y-1">
                {proxyIps.slice(-5).reverse().map((item) => (
                  <div key={`${item.ts}-${item.ip}`} className="flex items-center justify-between gap-2 text-[11px]">
                    <span className="truncate font-mono text-ink">{item.ip}</span>
                    <span className="shrink-0 text-ink-muted">{item.node ?? "—"}</span>
                    <span className="shrink-0 text-ink-muted">{relativeTime(new Date(item.ts * 1000).toISOString())}</span>
                  </div>
                ))}
                {proxyIps.length === 0 ? <div className="text-[10.5px] text-ink-muted">—</div> : null}
              </div>
            </div>
            <div>
              <div className="text-[11px] font-medium text-ink-soft">{t("analytics.campusTimeline")}</div>
              <div className="mt-1 flex flex-wrap gap-1">
                {(analytics?.campus ?? []).slice(-8).map((point) => (
                  <Badge key={point.ts} tone={point.onCampus ? "ok" : "muted"}>
                    {point.onCampus ? t("status.onCampus") : t("status.offCampus")}
                  </Badge>
                ))}
                {(analytics?.campus ?? []).length === 0 ? <span className="text-[10.5px] text-ink-muted">—</span> : null}
              </div>
            </div>
          </div>
        </Card>
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-3">
        <Card className="xl:col-span-2">
          <CardHeader title={t("analytics.aiTitle")} subtitle={t("analytics.aiHint")} icon={<Globe2 size={15} />} />
          <div className="flex flex-wrap gap-1.5">
            {aiDomains.map((domain) => (
              <span key={domain} className="chip inline-flex items-center gap-1.5 font-mono text-[10.5px]">
                {domain}
                <button
                  type="button"
                  className="text-ink-muted hover:text-[var(--err)]"
                  disabled={aiSaving}
                  onClick={() => void removeAiDomain(domain)}
                >
                  <X size={11} />
                </button>
              </span>
            ))}
            {aiDomains.length === 0 ? <span className="text-[11px] text-ink-muted">{t("analytics.noAiDomains")}</span> : null}
          </div>
          <div className="mt-3 flex items-end gap-2">
            <Field label={t("analytics.addDomain")} className="flex-1">
              <TextInput
                value={aiInput}
                onChange={(event) => setAiInput(event.target.value)}
                placeholder="chatgpt.com"
                className="font-mono text-[12px]"
              />
            </Field>
            <Button
              size="md"
              variant="secondary"
              icon={<Plus size={13} />}
              disabled={aiSaving || !aiInput.trim()}
              onClick={() => void addAiDomain()}
            >
              {t("common.add")}
            </Button>
            <Button
              size="md"
              variant="primary"
              icon={<Save size={13} />}
              disabled={aiSaving}
              onClick={() => void updateAiDomains(aiDomains)}
            >
              {t("common.save")}
            </Button>
          </div>
          <div className="mt-3 grid grid-cols-2 gap-2">
            <div className="rounded-lg border border-line bg-card px-2.5 py-2">
              <div className="text-[10.5px] text-ink-muted">{t("analytics.aiTraffic")}</div>
              <div className="mt-0.5 text-[15px] font-semibold text-ink">
                {totals ? formatBytes(totals.ai.up + totals.ai.down) : "—"}
              </div>
            </div>
            <div className="rounded-lg border border-line bg-card px-2.5 py-2">
              <div className="text-[10.5px] text-ink-muted">{t("analytics.egressCount")}</div>
              <div className="mt-0.5 text-[15px] font-semibold text-ink">{proxyIps.length}</div>
            </div>
          </div>
        </Card>

        <Card>
          <CardHeader title={t("analytics.aiIpTitle")} subtitle={t("analytics.aiIpHint")} icon={<Globe2 size={15} />} />
          <div className="max-h-[300px] space-y-1.5 overflow-y-auto scroll-slim pr-1">
            {proxyIps.length === 0 ? (
              <div className="text-[11px] text-ink-muted">{t("analytics.noData")}</div>
            ) : (
              [...proxyIps]
                .reverse()
                .slice(0, 20)
                .map((item) => (
                  <div
                    key={`${item.ts}-${item.ip}`}
                    className="flex items-center justify-between gap-2 rounded-lg border border-line bg-card px-2.5 py-1.5 text-[11px]"
                  >
                    <div className="min-w-0">
                      <div className="truncate font-mono text-ink">{item.ip}</div>
                      <div className="truncate text-[10px] text-ink-muted">{item.node ?? "—"}</div>
                    </div>
                    <span className="shrink-0 text-[10px] text-ink-muted">
                      {new Date(item.ts * 1000).toLocaleString()}
                    </span>
                  </div>
                ))
            )}
          </div>
        </Card>
      </div>
    </>
  );
}
