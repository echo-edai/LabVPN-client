import { ArrowDown, ArrowUp, Activity } from "lucide-react";
import { useMemo } from "react";

import { Card, CardHeader } from "@/components/ui";
import { MAX_TRAFFIC_POINTS } from "@/lib/constants";
import { formatBytes, formatSpeed } from "@/lib/format";
import { useAppStore } from "@/stores/app";

export function TrafficChart() {
  const t = useAppStore((state) => state.t);
  const traffic = useAppStore((state) => state.traffic);
  const running = useAppStore((state) => state.engine?.running ?? false);

  const current = traffic[traffic.length - 1];

  const paths = useMemo(() => {
    const points = traffic.slice(-MAX_TRAFFIC_POINTS);
    const width = 640;
    const height = 120;
    const maxValue = Math.max(1, ...points.map((point) => Math.max(point.down, point.up)));
    if (points.length < 2) {
      return { line: "", area: "", upLine: "" };
    }
    const stepX = width / (points.length - 1);
    const toY = (value: number) => height - (value / maxValue) * (height - 14) - 6;
    const downPoints = points.map((point, index) => [index * stepX, toY(point.down)] as const);
    const upPoints = points.map((point, index) => [index * stepX, toY(point.up)] as const);
    const line = downPoints.map(([x, y], index) => `${index === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`).join(" ");
    const area = `${line} L${width},${height} L0,${height} Z`;
    const upLine = upPoints.map(([x, y], index) => `${index === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`).join(" ");
    return { line, area, upLine };
  }, [traffic]);

  return (
    <Card>
      <CardHeader
        title={t("dashboard.traffic")}
        subtitle={t("dashboard.trafficHint")}
        icon={<Activity size={15} />}
        actions={
          <div className="flex items-center gap-3 text-[11.5px]">
            <span className="inline-flex items-center gap-1 text-[var(--accent)]">
              <ArrowDown size={12} />
              {formatSpeed(current?.down ?? 0)}
            </span>
            <span className="inline-flex items-center gap-1 text-ink-muted">
              <ArrowUp size={12} />
              {formatSpeed(current?.up ?? 0)}
            </span>
          </div>
        }
      />
      <div className="relative">
        {traffic.length < 2 ? (
          <div className="flex h-[120px] items-center justify-center text-[11.5px] text-ink-muted">
            {running ? "等待流量数据…" : t("status.disconnected")}
          </div>
        ) : (
          <svg viewBox="0 0 640 120" className="h-[120px] w-full" preserveAspectRatio="none">
            <defs>
              <linearGradient id="traffic-fill" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor="var(--accent)" stopOpacity="0.28" />
                <stop offset="100%" stopColor="var(--accent)" stopOpacity="0" />
              </linearGradient>
            </defs>
            {[0.25, 0.5, 0.75].map((ratio) => (
              <line
                key={ratio}
                x1="0"
                x2="640"
                y1={120 * ratio}
                y2={120 * ratio}
                stroke="var(--line)"
                strokeDasharray="3 5"
              />
            ))}
            <path d={paths.area} fill="url(#traffic-fill)" />
            <path d={paths.line} fill="none" stroke="var(--accent)" strokeWidth="1.8" strokeLinecap="round" />
            <path d={paths.upLine} fill="none" stroke="var(--brand)" strokeWidth="1.2" strokeDasharray="3 3" opacity="0.7" />
          </svg>
        )}
      </div>
      <div className="mt-2 flex items-center justify-between font-mono text-[10px] text-ink-muted">
        <span>{running ? "live" : "paused"}</span>
        <span>
          ↓ {formatBytes(current?.downTotal ?? 0)} total · ↑ {formatBytes(current?.upTotal ?? 0)}
        </span>
      </div>
    </Card>
  );
}
