import { Activity, Clock, Cpu, Globe2, GraduationCap, Plus, RefreshCw, Save, ShieldCheck, TerminalSquare, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { PageHeader } from "@/components/layout/PageHeader";
import { Badge, Button, Card, CardHeader, EmptyState, Progress, Segmented, TextInput } from "@/components/ui";
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
  const aiExposure = useAppStore((state) => state.aiExposure);
  const loadAiExposure = useAppStore((state) => state.loadAiExposure);
  const range = useAppStore((state) => state.analyticsRange);
  const loadAnalytics = useAppStore((state) => state.loadAnalytics);
  const updateAiDomains = useAppStore((state) => state.updateAiDomains);
  const engine = useAppStore((state) => state.engine);
  const [aiInput, setAiInput] = useState("");
  const [aiSaving, setAiSaving] = useState(false);
  const [selectedDomain, setSelectedDomain] = useState("");

  useEffect(() => {
    void loadAnalytics();
    void loadAiExposure();
  }, [loadAnalytics, loadAiExposure]);

  useEffect(() => {
    if (!selectedDomain && aiExposure?.domains.length) {
      setSelectedDomain(aiExposure.domains[0]);
    }
  }, [aiExposure, selectedDomain]);

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
          onChange={(value) => {
            void loadAnalytics(value as Range);
            void loadAiExposure(value as Range);
          }}
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
          <CardHeader
            title={t("analytics.aiExposureTitle")}
            subtitle={t("analytics.aiExposureHint")}
            icon={<Globe2 size={15} />}
            actions={
              <Button size="sm" variant="secondary" icon={<RefreshCw size={12} />} onClick={() => void loadAiExposure()}>
                {t("common.refresh")}
              </Button>
            }
          />
          <div className="grid grid-cols-1 gap-4 lg:grid-cols-[200px_1fr]">
            <div className="space-y-1">
              {(aiExposure?.domains ?? []).map((domain) => {
                const series = aiExposure?.series.find((item) => item.domain === domain);
                const recordCount = series?.records.length ?? 0;
                const ipCount = new Set(series?.segments.map((segment) => segment.ip) ?? []).size;
                return (
                  <button
                    key={domain}
                    type="button"
                    onClick={() => setSelectedDomain(domain)}
                    className={`flex w-full items-center justify-between gap-1 rounded-lg px-2.5 py-1.5 text-left text-[11.5px] transition ${
                      selectedDomain === domain
                        ? "bg-[var(--brand-soft)] font-medium text-brand"
                        : "text-ink-soft hover:bg-card"
                    }`}
                  >
                    <span className="truncate font-mono">{domain}</span>
                    <span className="shrink-0 text-[10px] text-ink-muted">
                      {recordCount > 0 ? `${recordCount}条/${ipCount}IP` : "—"}
                    </span>
                  </button>
                );
              })}
              {(aiExposure?.domains ?? []).length === 0 ? (
                <div className="text-[11px] text-ink-muted">{t("analytics.noAiDomains")}</div>
              ) : null}
            </div>

            <div className="min-w-0">
              {(() => {
                const series = aiExposure?.series.find((item) => item.domain === selectedDomain);
                const segments = series?.segments ?? [];
                if (!series || segments.length === 0) {
                  return (
                    <EmptyState
                      icon={<Clock size={22} />}
                      title={t("analytics.aiNoRecords")}
                      hint={t("analytics.aiNoRecordsHint")}
                    />
                  );
                }
                const rows: { key: string; ip: string; node: string | null; index: number }[] = [];
                for (const segment of segments) {
                  const key = `${segment.ip}|${segment.node ?? ""}`;
                  if (!rows.some((row) => row.key === key)) {
                    rows.push({ key, ip: segment.ip, node: segment.node, index: rows.length });
                  }
                }
                const nowTs = Math.floor(Date.now() / 1000);
                const startTs = Math.min(...segments.map((segment) => segment.firstTs));
                const span = Math.max(60, nowTs - startTs);
                const width = 640;
                const rowHeight = 30;
                const chartHeight = rows.length * rowHeight + 20;
                const colors = ["#38bdf8", "#a78bfa", "#34d399", "#fbbf24", "#f472b6", "#60a5fa", "#f59e0b"];
                const colorOf = (index: number) => colors[index % colors.length];
                const lastSegment = segments[segments.length - 1];
                const uniqueIps = new Set(segments.map((segment) => segment.ip)).size;
                const uniqueNodes = new Set(segments.map((segment) => segment.node ?? "—")).size;
                return (
                  <>
                    <div className="mb-2 flex flex-wrap items-center gap-3 text-[10.5px] text-ink-muted">
                      <Badge tone="brand" className="!text-[10px]">
                        {selectedDomain}
                      </Badge>
                      <span>
                        {t("analytics.aiLastIp")}:{" "}
                        <span className="font-mono text-ink">{lastSegment.ip}</span>
                        {lastSegment.node ? ` · ${lastSegment.node}` : ""}
                      </span>
                      <span>
                        {t("analytics.aiIpCount")}: <span className="text-ink">{uniqueIps}</span>
                      </span>
                      <span>
                        {t("analytics.aiNodeCount")}: <span className="text-ink">{uniqueNodes}</span>
                      </span>
                      <span>
                        {t("analytics.aiRecordCount")}: <span className="text-ink">{series.records.length}</span>
                      </span>
                    </div>
                    <div className="overflow-x-auto">
                      <svg viewBox={`0 0 ${width} ${chartHeight}`} className="h-auto w-full min-w-[520px]">
                        {rows.map((row) => (
                          <g key={row.key}>
                            <text x={0} y={row.index * rowHeight + 14} fontSize="10" fill="var(--ink-muted)">
                              {row.ip}
                            </text>
                            <text x={150} y={row.index * rowHeight + 14} fontSize="9" fill="var(--ink-muted)" opacity={0.75}>
                              {row.node ?? "—"}
                            </text>
                            {segments
                              .filter((segment) => `${segment.ip}|${segment.node ?? ""}` === row.key)
                              .map((segment) => {
                                const x = 230 + ((segment.firstTs - startTs) / span) * (width - 240);
                                const segmentWidth = Math.max(2, ((segment.lastTs - segment.firstTs) / span) * (width - 240));
                                return (
                                  <rect
                                    key={`${segment.firstTs}-${segment.ip}`}
                                    x={x}
                                    y={row.index * rowHeight + 4}
                                    width={segmentWidth}
                                    height={rowHeight - 12}
                                    rx={2}
                                    fill={colorOf(row.index)}
                                    opacity={0.85}
                                  >
                                    <title>
                                      {`${segment.ip}${segment.node ? ` · ${segment.node}` : ""}
${new Date(
                                        segment.firstTs * 1000,
                                      ).toLocaleString()} → ${new Date(segment.lastTs * 1000).toLocaleString()}（${segment.count}次）`}
                                    </title>
                                  </rect>
                                );
                              })}
                          </g>
                        ))}
                        <line
                          x1={230}
                          x2={width}
                          y1={chartHeight - 10}
                          y2={chartHeight - 10}
                          stroke="var(--line)"
                          strokeDasharray="3 4"
                        />
                        <text x={230} y={chartHeight - 1} fontSize="9" fill="var(--ink-muted)">
                          {new Date(startTs * 1000).toLocaleDateString()}
                        </text>
                        <text x={width - 60} y={chartHeight - 1} fontSize="9" fill="var(--ink-muted)">
                          现在
                        </text>
                      </svg>
                    </div>
                    <div className="mt-2 max-h-[150px] space-y-1 overflow-y-auto scroll-slim pr-1">
                      {series.records.slice(0, 20).map((record) => (
                        <div
                          key={`${record.ts}-${record.ip}-${record.host}`}
                          className="flex items-center justify-between gap-2 rounded-lg border border-line bg-card px-2.5 py-1 text-[10.5px]"
                        >
                          <span className="w-[120px] shrink-0 text-ink-muted">
                            {new Date(record.ts * 1000).toLocaleString()}
                          </span>
                          <span className="min-w-0 flex-1 truncate font-mono text-ink">{record.ip ?? "未知"}</span>
                          <span className="w-[130px] shrink-0 truncate text-right text-ink-muted">
                            {record.node ?? "—"}
                          </span>
                          <span className="w-[150px] shrink-0 truncate text-right font-mono text-[10px] text-ink-muted">
                            {record.host}
                          </span>
                        </div>
                      ))}
                    </div>
                  </>
                );
              })()}
            </div>
          </div>
          <div className="mt-3 flex flex-wrap items-center gap-2 border-t border-line pt-3">
            <TextInput
              value={aiInput}
              onChange={(event) => setAiInput(event.target.value)}
              placeholder="chatgpt.com"
              className="min-w-[180px] flex-1 font-mono text-[12px]"
            />
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
              variant="secondary"
              icon={<X size={13} />}
              disabled={aiSaving || !selectedDomain}
              onClick={() => void removeAiDomain(selectedDomain)}
            >
              {t("analytics.removeDomain")}
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
        </Card>

        <Card>
          <CardHeader title={t("analytics.aiPrincipleTitle")} subtitle={t("analytics.aiPrincipleHint")} icon={<ShieldCheck size={15} />} />
          <ul className="space-y-2.5 text-[11px] leading-5 text-ink-muted">
            <li>· 每次检测到对 AI 平台域名的连接，记录：时间、命中域名、代理链最终节点、该节点当时的出口 IP。</li>
            <li>· 出口 IP 通过混合代理端口实时查询（平台看到的即该 IP）；节点切换会立即产生新记录。</li>
            <li>· 同一平台同一节点 60 秒内只记一次，避免刷屏；无流量时不会产生记录。</li>
            <li>· 数据仅保存在本机 <span className="font-mono">stats/ai-exposure-*.jsonl</span>。</li>
            <li>· 如出口 IP 暂时查询失败，记录为「未知」，后续访问会补记。</li>
          </ul>
          <div className="mt-3 rounded-lg border border-line bg-card px-2.5 py-2 text-[10.5px] leading-4 text-ink-muted">
            {t("analytics.aiPrincipleFooter")}
          </div>
        </Card>
      </div>
    </>
  );
}
