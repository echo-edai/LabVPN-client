import type { ReactNode } from "react";

import { Sidebar } from "@/components/layout/Sidebar";
import { ToastHost } from "@/components/ui";

export function AppShell({ children }: { children: ReactNode }) {
  return (
    <div className="flex h-full w-full overflow-hidden">
      <Sidebar />
      <main className="relative flex-1 overflow-hidden">
        <div className="scroll-slim h-full overflow-y-auto">
          <div className="mx-auto w-full max-w-[1200px] px-7 pb-10 pt-6">{children}</div>
        </div>
      </main>
      <ToastHost />
    </div>
  );
}
