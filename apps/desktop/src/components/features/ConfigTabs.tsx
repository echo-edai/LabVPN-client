import { CloudDownload, GraduationCap, Route, TerminalSquare } from "lucide-react";
import { NavLink } from "react-router-dom";

import { ROUTES } from "@/lib/constants";
import { cn } from "@/lib/cn";
import { useAppStore } from "@/stores/app";

/**
 * 并行配置导航：订阅 / 校内 VPN / SSH 转发 三项配置相互独立，
 * 可同时启用、可只启用其一，不存在先后顺序。
 */
const tabs = [
  { to: ROUTES.setupSubscription, labelKey: "nav.setupSubscription" as const, icon: CloudDownload, flag: "subscription" },
  { to: ROUTES.setupCampus, labelKey: "nav.setupCampus" as const, icon: GraduationCap, flag: "campus" },
  { to: ROUTES.setupSsh, labelKey: "nav.setupSsh" as const, icon: TerminalSquare, flag: "ssh" },
  { to: ROUTES.merge, labelKey: "wizard.stepMerge" as const, icon: Route, flag: "merge" },
];

export function ConfigTabs() {
  const t = useAppStore((state) => state.t);
  const config = useAppStore((state) => state.config);

  const enabled: Record<string, boolean> = {
    subscription: Boolean(config?.subscription.enabled && config?.subscription.url.trim()),
    campus: Boolean(config?.campusVpn.enabled),
    ssh: Boolean(config?.sshProxy.enabled),
    merge: true,
  };

  return (
    <div className="mb-5 rounded-2xl border border-line bg-card px-2.5 py-2 backdrop-blur">
      <div className="flex flex-wrap items-center gap-1">
        <span className="mr-1 hidden text-[10px] font-semibold uppercase tracking-[0.14em] text-ink-muted sm:inline">
          {t("configTabs.parallel")}
        </span>
        {tabs.map(({ to, labelKey, icon: Icon, flag }) => (
          <NavLink
            key={to}
            to={to}
            className={({ isActive }) =>
              cn(
                "flex items-center gap-1.5 rounded-xl px-2.5 py-1.5 text-[12px] transition",
                isActive ? "bg-[var(--brand-soft)] font-semibold text-ink" : "text-ink-soft hover:bg-card-strong",
              )
            }
          >
            <Icon size={13.5} strokeWidth={1.9} />
            {t(labelKey)}
            {flag !== "merge" ? (
              <span
                className={cn(
                  "ml-0.5 h-1.5 w-1.5 rounded-full",
                  enabled[flag] ? "bg-[var(--ok)]" : "bg-ink-muted/40",
                )}
                title={enabled[flag] ? t("common.enabled") : t("common.disabled")}
              />
            ) : null}
          </NavLink>
        ))}
      </div>
      <div className="mt-1 px-1 text-[10.5px] leading-4 text-ink-muted">{t("configTabs.hint")}</div>
    </div>
  );
}
