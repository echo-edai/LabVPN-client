import { ArrowDownToLine, ExternalLink, Sparkles } from "lucide-react";

import { Button, Modal } from "@/components/ui";
import { useAppStore } from "@/stores/app";

/** 应用内更新提示：展示最新版本与发布说明，由用户决定「前往下载 / 稍后 / 忽略此版本」。 */
export function UpdateDialog() {
  const t = useAppStore((state) => state.t);
  const open = useAppStore((state) => state.updateDialogOpen);
  const update = useAppStore((state) => state.update);
  const close = useAppStore((state) => state.closeUpdateDialog);
  const skip = useAppStore((state) => state.skipUpdate);
  const openPath = useAppStore((state) => state.openPath);
  const pushToast = useAppStore((state) => state.pushToast);

  if (!update || !update.hasUpdate) return null;

  const notes = (update.notes ?? "").trim();
  const published = update.publishedAt
    ? new Date(update.publishedAt).toLocaleString()
    : null;

  return (
    <Modal
      open={open}
      title={
        <span className="flex items-center gap-2">
          <Sparkles size={15} className="text-[var(--brand)]" />
          {t("update.dialogTitle", { version: update.latest })}
        </span>
      }
      onClose={close}
      footer={
        <>
          <Button size="sm" variant="ghost" onClick={() => void skip(update.latest)}>
            {t("update.skip")}
          </Button>
          <Button size="sm" variant="secondary" onClick={close}>
            {t("update.later")}
          </Button>
          <Button
            size="sm"
            variant="primary"
            icon={<ArrowDownToLine size={13} />}
            onClick={async () => {
              try {
                await openPath(update.releaseUrl);
              } catch {
                pushToast("error", t("update.openFailed"));
              }
              close();
            }}
          >
            {t("update.download")}
          </Button>
        </>
      }
    >
      <div className="space-y-2.5">
        <p>
          {t("update.dialogBody", { current: update.current, latest: update.latest })}
        </p>
        {published ? (
          <p className="text-[11px] text-ink-muted">
            {t("update.publishedAt", { date: published })}
            {update.prerelease ? ` · ${t("update.prerelease")}` : ""}
          </p>
        ) : null}
        {notes ? (
          <pre className="scroll-slim max-h-56 overflow-y-auto whitespace-pre-wrap rounded-lg border border-line bg-card p-2.5 font-mono text-[10.5px] leading-4 text-ink-soft">
            {notes}
          </pre>
        ) : null}
        <a
          href={update.releaseUrl}
          target="_blank"
          rel="noreferrer"
          className="inline-flex items-center gap-1 text-[11px] text-brand hover:underline"
        >
          <ExternalLink size={11} />
          {t("update.releasePage")}
        </a>
      </div>
    </Modal>
  );
}
