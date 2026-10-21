import {
  GraduationCap,
  LayoutDashboard,
  Route,
  ScrollText,
  Settings,
  TerminalSquare,
  CloudDownload,
  ShieldCheck,
} from "lucide-react";
import { NavLink, useLocation } from "react-router-dom";

import { Logo } from "@/components/layout/Logo";
import { StatusDot } from "@/components/ui";
import { APP_VERSION, ROUTES } from "@/lib/constants";
import { cn } from "@/lib/cn";
import { useAppStore } from "@/stores/app";

const wizardItems = [
  { to: ROUTES.setupSubscription, labelKey: "nav.setupSubscription" as const, icon: CloudDownload },
  { to: ROUTES.setupCampus, labelKey: "nav.setupCampus" as const, icon: GraduationCap },
  { to: ROUTES.setupSsh, labelKey: "nav.setupSsh" as const, icon: TerminalSquare },
];

export function Sidebar() {
  const t = useAppStore((state) => state.t);
  const engine = useAppStore((state) => state.engine);
  const config = useAppStore((state) => state.config);
  const location = useLocation();
  const ready = config?.ui.setupCompleted ?? false;

  const NavItem = ({
    to,
    label,
    icon: Icon,
    badge,
  }: {
    to: string;
    label: string;
    icon: typeof LayoutDashboard;
    badge?: string;
  }) => (
    <NavLink
      to={to}
      className={({ isActive }) => cn("nav-item", (isActive || (to !== "/" && location.pathname.startsWith(to))) && "active")}
    >
      <Icon size={16} strokeWidth={1.9} />
      <span className="flex-1 truncate">{label}</span>
      {badge ? <span className="chip !px-1.5 !py-0 text-[10px]">{badge}</span> : null}
    </NavLink>
  );

  return (
    <aside className="flex h-full w-[236px] shrink-0 flex-col border-r border-line bg-[var(--bg-elev)]/45 px-3 py-4 backdrop-blur-xl">
      <div className="mb-5 flex items-center gap-2.5 px-2">
        <Logo size={34} />
        <div className="min-w-0">
          <div className="text-[15px] font-semibold tracking-tight text-ink">LabVPN</div>
          <div className="truncate text-[10.5px] text-ink-muted">{t("common.tagline")}</div>
        </div>
      </div>

      <nav className="flex flex-1 flex-col gap-0.5 overflow-y-auto">
        <NavItem to={ROUTES.dashboard} label={t("nav.dashboard")} icon={LayoutDashboard} />
        <div className="mt-3 mb-1 px-2 text-[10px] font-semibold uppercase tracking-[0.16em] text-ink-muted">
          {t("configTabs.parallel")}
          {!ready ? (
            <span className="ml-1.5 rounded bg-[var(--warn)]/15 px-1 py-0.5 text-[9px] normal-case text-[var(--warn)]">
              {t("nav.firstRun")}
            </span>
          ) : null}
        </div>
        {wizardItems.map((item) => (
          <NavItem key={item.to} to={item.to} label={t(item.labelKey)} icon={item.icon} />
        ))}
        <div className="mt-3 mb-1 px-2 text-[10px] font-semibold uppercase tracking-[0.16em] text-ink-muted">
          {t("nav.merge")}
        </div>
        <NavItem to={ROUTES.merge} label={t("nav.merge")} icon={Route} />
        <NavItem to={ROUTES.logs} label={t("nav.logs")} icon={ScrollText} />
        <NavItem to={ROUTES.settings} label={t("nav.settings")} icon={Settings} />
      </nav>

      <div className="mt-3 rounded-xl border border-line bg-card p-3">
        <div className="flex items-center gap-2">
          <StatusDot tone={engine?.running ? "ok" : "muted"} pulse={engine?.running ?? false} />
          <span className="text-[11.5px] font-medium text-ink-soft">
            {engine?.running ? t("status.connected") : t("status.disconnected")}
          </span>
          <ShieldCheck size={13} className={engine?.running ? "ml-auto text-[var(--ok)]" : "ml-auto text-ink-muted"} />
        </div>
        <div className="mt-1.5 flex items-center justify-between text-[10.5px] text-ink-muted">
          <span>{engine?.mode === "tun" ? t("mode.tun") : engine?.running ? t("mode.systemProxy") : t("status.idle")}</span>
          <span>v{APP_VERSION}</span>
        </div>
      </div>
    </aside>
  );
}
