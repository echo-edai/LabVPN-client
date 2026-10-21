/** 格式化工具。 */

export function formatBytes(bytes: number, digits = 1): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** index;
  return `${value.toFixed(index === 0 ? 0 : digits)} ${units[index]}`;
}

export function formatSpeed(bytesPerSecond: number): string {
  return `${formatBytes(bytesPerSecond)}/s`;
}

export function formatDuration(totalSeconds: number): string {
  if (!Number.isFinite(totalSeconds) || totalSeconds < 0) return "—";
  const seconds = Math.floor(totalSeconds % 60);
  const minutes = Math.floor((totalSeconds / 60) % 60);
  const hours = Math.floor(totalSeconds / 3600);
  if (hours > 0) return `${hours} 小时 ${minutes} 分`;
  if (minutes > 0) return `${minutes} 分 ${seconds} 秒`;
  return `${seconds} 秒`;
}

export function formatPercent(ratio: number): string {
  if (!Number.isFinite(ratio)) return "—";
  return `${Math.round(Math.min(1, Math.max(0, ratio)) * 100)}%`;
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export interface TrafficUsage {
  used: number;
  total: number;
  ratio: number;
  expireText: string;
}

export function trafficUsage(
  traffic: { upload: number; download: number; total: number; expire: number | null } | null,
): TrafficUsage | null {
  if (!traffic) return null;
  const used = traffic.upload + traffic.download;
  const ratio = traffic.total > 0 ? used / traffic.total : 0;
  let expireText = "长期有效";
  if (traffic.expire) {
    const expireDate = new Date(traffic.expire * 1000);
    const days = Math.ceil((expireDate.getTime() - Date.now()) / 86_400_000);
    expireText =
      days > 0
        ? `${expireDate.toLocaleDateString()} · 剩余 ${days} 天`
        : `${expireDate.toLocaleDateString()} · 已过期`;
  }
  return { used, total: traffic.total, ratio, expireText };
}

export function relativeTime(input: string | null | undefined): string {
  if (!input) return "从未";
  const time = new Date(input).getTime();
  if (Number.isNaN(time)) return input;
  const diff = Date.now() - time;
  const minutes = Math.round(diff / 60_000);
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  const days = Math.round(hours / 24);
  if (days < 30) return `${days} 天前`;
  return new Date(time).toLocaleDateString();
}

export function formatClock(ts: string): string {
  return ts.length > 8 ? ts.slice(-8) : ts;
}

export function truncateMiddle(input: string, max = 42): string {
  if (input.length <= max) return input;
  const half = Math.floor((max - 1) / 2);
  return `${input.slice(0, half)}…${input.slice(-half)}`;
}

export function newId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}
