import { Check } from "lucide-react";
import { useNavigate } from "react-router-dom";

import { ROUTES } from "@/lib/constants";
import { cn } from "@/lib/cn";
import { useAppStore } from "@/stores/app";

const steps = [
  { index: 1, route: ROUTES.setupSubscription, key: "wizard.stepSubscription" as const },
  { index: 2, route: ROUTES.setupCampus, key: "wizard.stepCampus" as const },
  { index: 3, route: ROUTES.setupSsh, key: "wizard.stepSsh" as const },
  { index: 4, route: ROUTES.merge, key: "wizard.stepMerge" as const },
];

export function WizardStepper({ current }: { current: number }) {
  const t = useAppStore((state) => state.t);
  const navigate = useNavigate();

  return (
    <div className="mb-6 flex items-center gap-1 overflow-x-auto rounded-2xl border border-line bg-card px-2 py-2 backdrop-blur">
      {steps.map((step, index) => {
        const done = step.index < current;
        const active = step.index === current;
        return (
          <div key={step.index} className="flex flex-1 items-center gap-1">
            <button
              type="button"
              onClick={() => navigate(step.route)}
              className={cn(
                "flex flex-1 items-center gap-2 rounded-xl px-2.5 py-1.5 text-left transition",
                active ? "bg-[var(--brand-soft)]" : "hover:bg-card-strong",
              )}
            >
              <span
                className={cn(
                  "flex h-6 w-6 shrink-0 items-center justify-center rounded-full border text-[11px] font-semibold",
                  done && "border-transparent bg-[var(--ok)]/90 text-white",
                  active && !done && "border-transparent bg-gradient-to-br from-[var(--brand)] to-[var(--accent)] text-white",
                  !done && !active && "border-line text-ink-muted",
                )}
              >
                {done ? <Check size={12} /> : step.index}
              </span>
              <span className="min-w-0">
                <span className="block text-[9.5px] uppercase tracking-wider text-ink-muted">
                  STEP {step.index}
                </span>
                <span className={cn("block truncate text-[12px]", active ? "font-semibold text-ink" : "text-ink-soft")}>
                  {t(step.key)}
                </span>
              </span>
            </button>
            {index < steps.length - 1 ? <span className="h-px w-4 shrink-0 bg-line" /> : null}
          </div>
        );
      })}
    </div>
  );
}
