import { Copy, Eraser, RefreshCw, ScrollText, Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { PageHeader } from "@/components/layout/PageHeader";
import { Button, Card, EmptyState, Segmented, TextInput, type SegmentedOption } from "@/components/ui";
import { cn } from "@/lib/cn";
import { useAppStore } from "@/stores/app";

/** 日志页来源切换：校内 VPN 单独一页，便于定位 RVPN 侧问题。 */
type LogTab = "all" | "vpn" | "core" | "engine" | "other";

const CORE_SOURCES = ["vpn", "core", "engine"] as const;

const levelColor: Record<string, string> = {
  info: "text-ink-muted",
  warn: "text-[var(--warn)]",
  error: "text-[var(--err)]",
};

export function Logs() {
  const t = useAppStore((state) => state.t);
  const logs = useAppStore((state) => state.logs);
  const loadLogs = useAppStore((state) => state.loadLogs);
  const clearLogs = useAppStore((state) => state.clearLogs);
  const pushToast = useAppStore((state) => state.pushToast);
  const [tab, setTab] = useState<LogTab>("all");
  const [level, setLevel] = useState<"all" | "info" | "warn" | "error">("all");
  const [query, setQuery] = useState("");

  useEffect(() => {
    void loadLogs(500);
    const timer = setInterval(() => void loadLogs(500), 2000);
    return () => clearInterval(timer);
  }, [loadLogs]);

  const tabs: SegmentedOption<LogTab>[] = [
    { value: "all", label: t("logs.tabAll") },
    { value: "vpn", label: t("logs.tabVpn") },
    { value: "core", label: t("logs.tabCore") },
    { value: "engine", label: t("logs.tabEngine") },
    { value: "other", label: t("logs.tabOther") },
  ];

  const filtered = useMemo(
    () =>
      logs.filter((entry) => {
        if (tab === "vpn" && entry.source !== "vpn") return false;
        if (tab === "core" && entry.source !== "core") return false;
        if (tab === "engine" && entry.source !== "engine") return false;
        if (tab === "other" && (CORE_SOURCES as readonly string[]).includes(entry.source)) {
          return false;
        }
        if (level !== "all" && entry.level !== level) return false;
        if (query && !entry.message.toLowerCase().includes(query.toLowerCase())) return false;
        return true;
      }),
    [logs, tab, level, query],
  );

  const copyAll = async () => {
    const text = filtered
      .map((entry) => `[${entry.ts}] [${entry.level}] [${entry.source}] ${entry.message}`)
      .join("\n");
    try {
      await navigator.clipboard.writeText(text);
      pushToast("success", t("toast.copied"));
    } catch {
      pushToast("error", t("toast.failed"));
    }
  };

  return (
    <>
      <PageHeader
        eyebrow="OBSERVABILITY"
        title={t("logs.title")}
        subtitle={t("logs.subtitle")}
        actions={
          <>
            <Button size="sm" variant="secondary" icon={<RefreshCw size={13} />} onClick={() => void loadLogs(500)}>
              {t("common.refresh")}
            </Button>
            <Button size="sm" variant="ghost" icon={<Copy size={13} />} onClick={() => void copyAll()}>
              {t("logs.copy")}
            </Button>
            <Button size="sm" variant="ghost" icon={<Eraser size={13} />} onClick={() => void clearLogs()}>
              {t("logs.clear")}
            </Button>
          </>
        }
      />

      <Card className="p-4">
        <div className="mb-3 flex flex-wrap items-center gap-2">
          <Segmented size="sm" value={tab} onChange={setTab} options={tabs} />
          <span className="text-[10.5px] text-ink-muted">{filtered.length} 条</span>
        </div>

        <div className="mb-3 flex flex-wrap items-center gap-2">
          <div className="relative min-w-[180px] flex-1">
            <Search size={13} className="absolute left-3 top-1/2 -translate-y-1/2 text-ink-muted" />
            <TextInput
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="搜索日志…"
              className="pl-8"
            />
          </div>
          <Segmented
            value={level}
            onChange={setLevel}
            options={[
              { value: "all", label: t("logs.levelAll") },
              { value: "info", label: "info" },
              { value: "warn", label: "warn" },
              { value: "error", label: "error" },
            ]}
          />
        </div>

        {tab === "vpn" && (
          <div className="mb-3 rounded-lg border border-line/60 bg-card-strong/40 px-3 py-2 text-[10.5px] leading-relaxed text-ink-muted">
            {t("logs.vpnHint")}
          </div>
        )}

        {filtered.length === 0 ? (
          <EmptyState icon={<ScrollText size={22} />} title={t("logs.empty")} />
        ) : (
          <div className="scroll-slim max-h-[calc(100vh-320px)] space-y-0.5 overflow-y-auto pr-1 font-mono text-[11px] leading-5">
            {[...filtered].reverse().map((entry) => (
              <div key={entry.id} className="flex gap-2 rounded px-2 py-0.5 hover:bg-card">
                <span className="shrink-0 text-ink-muted">{entry.ts}</span>
                <span className={cn("w-[44px] shrink-0 uppercase", levelColor[entry.level] ?? "text-ink-muted")}>
                  {entry.level}
                </span>
                <span className="w-[86px] shrink-0 truncate text-brand/80">{entry.source}</span>
                <span className="min-w-0 flex-1 break-words text-ink-soft">{entry.message}</span>
              </div>
            ))}
          </div>
        )}
      </Card>
    </>
  );
}
