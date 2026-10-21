import { ScrollText } from "lucide-react";
import { Link } from "react-router-dom";

import { Card, CardHeader, EmptyState } from "@/components/ui";
import { ROUTES } from "@/lib/constants";
import { cn } from "@/lib/cn";
import { useAppStore } from "@/stores/app";

const levelClass: Record<string, string> = {
  info: "text-ink-muted",
  warn: "text-[var(--warn)]",
  error: "text-[var(--err)]",
};

export function RecentLogs({ limit = 7 }: { limit?: number }) {
  const logs = useAppStore((state) => state.logs);
  const t = useAppStore((state) => state.t);
  const recent = [...logs].slice(-limit).reverse();

  return (
    <Card>
      <CardHeader
        title={t("dashboard.recentLogs")}
        icon={<ScrollText size={15} />}
        actions={
          <Link to={ROUTES.logs} className="text-[11.5px] text-brand hover:underline">
            {t("dashboard.viewAll")} →
          </Link>
        }
      />
      {recent.length === 0 ? (
        <EmptyState title={t("logs.empty")} />
      ) : (
        <div className="space-y-1.5">
          {recent.map((entry) => (
            <div key={entry.id} className="flex items-start gap-2 font-mono text-[10.5px] leading-4">
              <span className="shrink-0 text-ink-muted">{entry.ts}</span>
              <span className={cn("w-[38px] shrink-0 truncate", levelClass[entry.level] ?? "text-ink-muted")}>
                {entry.source}
              </span>
              <span className="min-w-0 flex-1 truncate text-ink-soft" title={entry.message}>
                {entry.message}
              </span>
            </div>
          ))}
        </div>
      )}
    </Card>
  );
}
