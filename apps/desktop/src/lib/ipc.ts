/**
 * IPC 统一封装：Tauri 环境走 `invoke`，浏览器预览走 mock。
 * 组件中禁止直接使用 `@tauri-apps/api`。
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { mockInvoke } from "@/lib/mock";
import type {
  AiExposureData,
  AnalyticsData,
  AppConfig,
  AppSnapshot,
  CampusStatus,
  ConfigEnvelope,
  ConnectOutcome,
  DiagnosticsResult,
  EngineStatus,
  ExistingVpn,
  LogEntry,
  OperationResult,
  PresetInfo,
  PreviewResult,
  ProxyGroupInfo,
  RuleValidation,
  SecretUpdates,
  SshTestResult,
  SubscriptionInfo,
  SystemInfo,
  UpdateInfo,
  ValidationIssue,
  VpnLogAnalysis,
  VpnStatus,
  VpnTestResult,
} from "@/lib/types";

/** 是否运行在 Tauri WebView 中。 */
export const isTauri =
  typeof window !== "undefined" && typeof window.__TAURI_INTERNALS__ !== "undefined";

export class IpcError extends Error {
  code: string;
  detail?: string;

  constructor(message: string, code = "unknown", detail?: string) {
    super(message);
    this.name = "IpcError";
    this.code = code;
    this.detail = detail;
  }
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri) {
    return (await mockInvoke(command, args)) as T;
  }
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw normalizeError(error);
  }
}

function normalizeError(error: unknown): IpcError {
  if (error instanceof IpcError) return error;
  if (typeof error === "object" && error !== null) {
    const record = error as Record<string, unknown>;
    const message = typeof record.message === "string" ? record.message : "请求失败";
    const code = typeof record.code === "string" ? record.code : "unknown";
    const detail = typeof record.detail === "string" ? record.detail : undefined;
    return new IpcError(message, code, detail);
  }
  if (typeof error === "string") return new IpcError(error);
  return new IpcError("请求失败");
}

export const api = {
  // 配置
  getSnapshot: () => call<AppSnapshot>("get_snapshot"),
  getConfig: () => call<ConfigEnvelope>("get_config"),
  saveConfig: (config: AppConfig, secrets: SecretUpdates = {}) =>
    call<ConfigEnvelope>("save_config", { config, secrets }),
  resetConfig: () => call<ConfigEnvelope>("reset_config"),
  exportConfig: () => call<string>("export_config"),
  importConfig: (json: string, secrets: SecretUpdates = {}) =>
    call<ConfigEnvelope>("import_config", { json, secrets }),
  validateConfig: (config: AppConfig) => call<ValidationIssue[]>("validate_config", { config }),
  previewConfig: () => call<PreviewResult>("preview_config"),
  getPresets: () => call<PresetInfo[]>("get_presets"),
  validateRules: (rules: string[]) => call<RuleValidation>("validate_rules", { rules }),
  diagnoseTarget: (input: string) => call<DiagnosticsResult>("diagnose_target", { input }),

  // 连接
  startEngine: () => call<ConnectOutcome>("start_engine"),
  stopEngine: () => call<EngineStatus>("stop_engine"),
  restartEngine: () => call<ConnectOutcome>("restart_engine"),
  applyConfig: () => call<ConnectOutcome | null>("apply_config"),
  engineStatus: () => call<EngineStatus>("engine_status"),
  getProxyGroups: () => call<ProxyGroupInfo[]>("get_proxy_groups"),
  getAnalytics: (range: string) => call<AnalyticsData>("get_analytics", { range }),
  getAiExposure: (range: string) => call<AiExposureData>("get_ai_exposure", { range }),
  setAiDomains: (domains: string[]) => call<string[]>("set_ai_domains", { domains }),
  getNodeCandidates: () => call<string[]>("get_node_candidates"),
  selectNode: (name: string) => call<OperationResult>("select_node", { name }),
  selectProxy: (group: string, name: string) =>
    call<OperationResult>("select_proxy", { group, name }),
  testProxyDelay: (name: string) => call<number>("test_proxy_delay", { name }),

  // 校园网
  getCampusStatus: (refresh = false) => call<CampusStatus>("get_campus_status", { refresh }),
  setCampusOverride: (value: boolean | null) =>
    call<CampusStatus>("set_campus_override", { value }),
  getCampusOverride: () => call<boolean | null>("campus_override"),
  refreshCampus: () => call<CampusStatus>("refresh_campus"),
  detectCampusNow: () => call<OperationResult>("detect_campus_now"),

  // 订阅
  importSubscription: (url: string, name?: string) =>
    call<SubscriptionInfo>("import_subscription", {
      request: { url, name: name ?? null, persist: true },
    }),
  testSubscription: (url: string) => call<SubscriptionInfo>("test_subscription", { url }),
  refreshSubscription: () => call<SubscriptionInfo>("refresh_subscription"),
  getSubscription: () => call<SubscriptionInfo>("get_subscription"),
  syncSubscription: () => call<OperationResult>("sync_subscription"),

  // 校内 VPN
  detectVpnBinary: () => call<ExistingVpn>("detect_vpn_binary"),
  startVpn: () => call<VpnStatus>("start_vpn"),
  stopVpn: () => call<VpnStatus>("stop_vpn"),
  vpnStatus: () => call<VpnStatus>("vpn_status"),
  testVpnConnection: () => call<VpnTestResult>("test_vpn_connection"),
  analyzeVpnLogs: () => call<VpnLogAnalysis>("analyze_vpn_logs"),
  testSsh: (host: string, port: number) => call<SshTestResult>("test_ssh", { host, port }),

  // 系统
  getSystemInfo: () => call<SystemInfo>("get_system_info"),
  installHelper: () => call<OperationResult>("install_helper"),
  uninstallHelper: () => call<OperationResult>("uninstall_helper"),
  startHelperElevated: () => call<OperationResult>("start_helper_elevated"),
  setSystemProxy: (enabled: boolean) => call<OperationResult>("set_system_proxy", { enabled }),
  getAutostart: () => call<boolean>("get_autostart"),
  setAutostart: (enabled: boolean) => call<OperationResult>("set_autostart", { enabled }),
  getLogs: (limit = 200) => call<LogEntry[]>("get_logs", { limit }),
  clearLogs: () => call<OperationResult>("clear_logs"),
  openPath: (target: string) => call<OperationResult>("open_path", { target }),
  exportDiagnostics: () => call<OperationResult>("export_diagnostics"),

  // 更新
  checkUpdate: () => call<UpdateInfo>("check_update"),
  skipUpdateVersion: (version: string) =>
    call<ConfigEnvelope>("skip_update_version", { version }),
};

/** 订阅事件（浏览器预览下为空实现）。 */
export function onEvent<T>(event: string, handler: (payload: T) => void): UnlistenFn {
  if (!isTauri) return () => undefined;
  let unlisten: UnlistenFn | undefined;
  let disposed = false;
  void listen<T>(event, (incoming) => handler(incoming.payload)).then((fn) => {
    if (disposed) {
      fn();
    } else {
      unlisten = fn;
    }
  });
  return () => {
    disposed = true;
    unlisten?.();
  };
}
