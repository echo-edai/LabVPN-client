import { Gauge, Network, Zap } from "lucide-react";

import { Badge, Button, Card, CardHeader, EmptyState } from "@/components/ui";
import { GROUP } from "@/lib/constants";
import { useAppStore } from "@/stores/app";

/** 自动策略说明：让用户理解每个组的用途与是否自动切换。 */
function groupHint(name: string): string | undefined {
  if (name === GROUP.campus) {
    return "自动：校内直连 / 校外经校内 VPN（依据校园网检测自动切换，无需手动选择）";
  }
  if (name === GROUP.proxy) return "国外流量出口：按「融合与规则 → 代理节点」策略选择节点";
  if (name === GROUP.auto) return "延迟自动测速组：按自定义间隔自动切换最低延迟节点";
  if (name === GROUP.final) return "未命中任何规则的流量兜底";
  if (name === GROUP.ssh) return "SSH 转发出口：指定目标的 TCP 流量经 SSH 服务器转发";
  if (name === GROUP.adblock) return "广告/拦截规则命中的流量";
  return undefined;
}

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
                <div className="mt-0.5 truncate text-[11px] text-ink-muted">
                  {groupHint(group.name) ?? group.now ?? "—"}
                </div>
                {group.name === GROUP.campus ? (
                  <div className="mt-0.5 truncate text-[10.5px] text-ink-muted">
                    当前出口：{group.now ?? "—"}
                  </div>
                ) : null}
              </div>
              {group.name === GROUP.campus ? (
                <Badge tone="ok" className="shrink-0">自动切换</Badge>
              ) : group.kind === "Selector" ? (
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
