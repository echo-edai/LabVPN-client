import { Gauge, Network, Zap } from "lucide-react";

import { Badge, Button, Card, CardHeader, EmptyState } from "@/components/ui";
import { useAppStore } from "@/stores/app";

export function ProxyGroups({ limit }: { limit?: number }) {
  const t = useAppStore((state) => state.t);
  const groups = useAppStore((state) => state.proxyGroups);
  const running = useAppStore((state) => state.engine?.running ?? false);
  const selectProxy = useAppStore((state) => state.selectProxy);
  const testDelay = useAppStore((state) => state.testProxyDelay);
  const testing = useAppStore((state) => state.busy === "delay");

  const visible = limit ? groups.slice(0, limit) : groups;

  return (
    <Card>
      <CardHeader
        title={t("dashboard.proxyGroups")}
        subtitle={`${groups.length} groups`}
        icon={<Network size={15} />}
        actions={
          <Button size="sm" variant="ghost" icon={<Zap size={12} />} loading={testing} onClick={() => void testDelay()}>
            {t("common.test")}
          </Button>
        }
      />
      {!running ? (
        <EmptyState icon={<Gauge size={22} />} title={t("status.disconnected")} hint={t("mode.systemProxyHint")} />
      ) : (
        <div className="space-y-2">
          {visible.map((group) => (
            <div
              key={group.name}
              className="flex items-center gap-3 rounded-xl border border-line bg-card px-3 py-2.5"
            >
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="truncate text-[12.5px] font-medium text-ink">{group.name}</span>
                  <Badge tone="muted" className="!px-1.5 !py-0 !text-[9.5px]">
                    {group.kind}
                  </Badge>
                </div>
                <div className="mt-0.5 truncate text-[11px] text-ink-muted">{group.now ?? "—"}</div>
              </div>
              {group.kind === "Selector" ? (
                <select
                  className="input h-8 w-[190px] shrink-0 !py-0 text-[11.5px]"
                  value={group.now ?? ""}
                  onChange={(event) => void selectProxy(group.name, event.target.value)}
                >
                  {group.all.map((item) => (
                    <option key={item} value={item}>
                      {item}
                    </option>
                  ))}
                </select>
              ) : (
                <span className="shrink-0 font-mono text-[11px] text-[var(--ok)]">
                  {group.delay ? `${group.delay} ms` : "—"}
                </span>
              )}
            </div>
          ))}
        </div>
      )}
    </Card>
  );
}
