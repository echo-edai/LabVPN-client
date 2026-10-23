/** 全局状态（Zustand）：快照、事件订阅、动作与提示。 */
import { create } from "zustand";

import { EVENTS, MAX_TRAFFIC_POINTS } from "@/lib/constants";
import { newId } from "@/lib/format";
import { api, IpcError, isTauri, onEvent } from "@/lib/ipc";
import { mockTrafficPoint } from "@/lib/mock";
import { translate, type MessageKey } from "@/locales";
import type {
  AiExposureData,
  AnalyticsData,
  AppConfig,
  AppSnapshot,
  CampusStatus,
  ConfigEnvelope,
  CoreInfo,
  DiagnosticsResult,
  EngineStatus,
  ExistingVpn,
  Language,
  LogEntry,
  PresetInfo,
  PreviewResult,
  ProxyGroupInfo,
  RuleValidation,
  SecretUpdates,
  SshTestResult,
  SubscriptionInfo,
  SystemInfo,
  ThemeMode,
  TrafficPoint,
  UpdateInfo,
  VpnStatus,
  VpnTestResult,
} from "@/lib/types";

export interface ToastItem {
  id: string;
  kind: "success" | "error" | "info" | "warn";
  title: string;
  message?: string;
}

type ResolvedTheme = "dark" | "light";

function resolveTheme(mode: ThemeMode): ResolvedTheme {
  if (mode === "system") {
    if (typeof window !== "undefined" && window.matchMedia("(prefers-color-scheme: dark)").matches) {
      return "dark";
    }
    return "light";
  }
  return mode;
}

function applyThemeClass(theme: ResolvedTheme) {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  root.classList.toggle("dark", theme === "dark");
  root.classList.toggle("light", theme === "light");
}

export interface AppStore {
  ready: boolean;
  loading: boolean;
  busy: string | null;
  snapshot: AppSnapshot | null;
  config: AppConfig | null;
  envelope: ConfigEnvelope | null;
  engine: EngineStatus | null;
  vpn: VpnStatus | null;
  campus: CampusStatus | null;
  subscription: SubscriptionInfo | null;
  system: SystemInfo | null;
  core: CoreInfo | null;
  presets: PresetInfo[];
  logs: LogEntry[];
  proxyGroups: ProxyGroupInfo[];
  nodeCandidates: string[];
  analytics: AnalyticsData | null;
  aiExposure: AiExposureData | null;
  analyticsRange: "24h" | "7d" | "30d";
  traffic: TrafficPoint[];
  themeMode: ThemeMode;
  resolvedTheme: ResolvedTheme;
  language: Language;
  toasts: ToastItem[];
  preview: PreviewResult | null;
  update: UpdateInfo | null;
  updateChecking: boolean;
  updateDialogOpen: boolean;

  init: () => Promise<void>;
  t: (key: MessageKey, vars?: Record<string, string | number>) => string;
  refresh: () => Promise<void>;
  checkUpdate: (force?: boolean) => Promise<UpdateInfo | null>;
  skipUpdate: (version: string) => Promise<void>;
  openUpdateDialog: () => void;
  closeUpdateDialog: () => void;
  loadProxyGroups: () => Promise<void>;
  saveConfig: (config: AppConfig, secrets?: SecretUpdates) => Promise<boolean>;
  applyConfig: () => Promise<boolean>;
  connect: () => Promise<void>;
  disconnect: () => Promise<void>;
  restart: () => Promise<void>;
  refreshCampus: () => Promise<void>;
  setCampusOverride: (value: boolean | null) => Promise<void>;
  importSubscription: (url: string, name?: string) => Promise<boolean>;
  testSubscription: (url: string) => Promise<SubscriptionInfo | null>;
  refreshSubscription: () => Promise<void>;
  testVpn: () => Promise<VpnTestResult | null>;
  testSsh: (host: string, port: number) => Promise<SshTestResult | null>;
  selectProxy: (group: string, name: string) => Promise<void>;
  loadNodeCandidates: () => Promise<void>;
  chooseNode: (name: string) => Promise<boolean>;
  loadAnalytics: (range?: "24h" | "7d" | "30d") => Promise<void>;
  loadAiExposure: (range?: "24h" | "7d" | "30d") => Promise<void>;
  updateAiDomains: (domains: string[]) => Promise<boolean>;
  testProxyDelay: () => Promise<void>;
  detectVpnBinary: () => Promise<ExistingVpn | null>;
  installHelper: () => Promise<void>;
  uninstallHelper: () => Promise<void>;
  startHelperElevated: () => Promise<void>;
  setSystemProxy: (enabled: boolean) => Promise<void>;
  setAutostart: (enabled: boolean) => Promise<void>;
  setThemeMode: (mode: ThemeMode) => void;
  setLanguage: (language: Language) => void;
  pushToast: (kind: ToastItem["kind"], title: string, message?: string) => void;
  dismissToast: (id: string) => void;
  loadLogs: (limit?: number) => Promise<void>;
  clearLogs: () => Promise<void>;
  loadPreview: () => Promise<void>;
  diagnose: (input: string) => Promise<DiagnosticsResult | null>;
  validateRules: (rules: string[]) => Promise<RuleValidation | null>;
  exportDiagnostics: () => Promise<void>;
  openPath: (target: string) => Promise<void>;
  exportConfig: () => Promise<string | null>;
  resetConfig: () => Promise<boolean>;
}

let initialized = false;
let mockTrafficTimer: ReturnType<typeof setInterval> | null = null;

function messageOf(error: unknown): string {
  if (error instanceof IpcError) {
    return error.detail ? `${error.message}：${error.detail}` : error.message;
  }
  if (error instanceof Error) return error.message;
  return String(error);
}

export const useAppStore = create<AppStore>()((set, get) => ({
  ready: false,
  loading: true,
  busy: null,
  snapshot: null,
  config: null,
  envelope: null,
  engine: null,
  vpn: null,
  campus: null,
  subscription: null,
  system: null,
  core: null,
  presets: [],
  logs: [],
  proxyGroups: [],
  nodeCandidates: [],
  analytics: null,
  aiExposure: null,
  analyticsRange: "24h",
  traffic: [],
  themeMode: "dark",
  resolvedTheme: "dark",
  language: "zh-CN",
  toasts: [],
  preview: null,
  update: null,
  updateChecking: false,
  updateDialogOpen: false,

  init: async () => {
    if (initialized) return;
    initialized = true;
    set({ loading: true });
    try {
      const snapshot = await api.getSnapshot();
      const themeMode = snapshot.config.config.ui.theme;
      const language = snapshot.config.config.ui.language;
      const resolvedTheme = resolveTheme(themeMode);
      applyThemeClass(resolvedTheme);
      set({
        snapshot,
        ready: true,
        loading: false,
        config: snapshot.config.config,
        envelope: snapshot.config,
        engine: snapshot.engine,
        vpn: snapshot.vpn,
        campus: snapshot.campus,
        subscription: snapshot.subscription,
        system: snapshot.system,
        core: snapshot.core,
        presets: snapshot.presets,
        logs: snapshot.logs,
        themeMode,
        resolvedTheme,
        language,
      });

      onEvent<EngineStatus>(EVENTS.engineStatus, (engine) => {
        set({ engine });
        if (engine.running) {
          void get().loadProxyGroups();
        }
      });
      onEvent<VpnStatus>(EVENTS.vpnStatus, (vpn) => set({ vpn }));
      onEvent<CampusStatus>(EVENTS.campusStatus, (campus) => set({ campus }));
      onEvent<TrafficPoint>(EVENTS.traffic, (point) => {
        set((state) => ({
          traffic: [...state.traffic, point].slice(-MAX_TRAFFIC_POINTS),
        }));
      });

      if (window.matchMedia) {
        window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
          if (get().themeMode === "system") {
            const theme = resolveTheme("system");
            applyThemeClass(theme);
            set({ resolvedTheme: theme });
          }
        });
      }

      if (!isTauri) {
        // 浏览器预览：预填历史流量 + 每秒追加，便于图表预览
        const prefill: TrafficPoint[] = [];
        let previous: TrafficPoint | undefined;
        for (let index = 0; index < 45; index += 1) {
          previous = mockTrafficPoint(previous);
          prefill.push(previous);
        }
        set({ traffic: prefill });
        mockTrafficTimer = setInterval(() => {
          const state = get();
          if (!state.engine?.running) return;
          const previous = state.traffic[state.traffic.length - 1];
          set((current) => ({
            traffic: [...current.traffic, mockTrafficPoint(previous)].slice(-MAX_TRAFFIC_POINTS),
          }));
        }, 1000);
      }
      if (snapshot.engine.running) {
        void get().loadProxyGroups();
      }
      // 启动后静默检查更新（不打扰用户；失败只在手动检查时提示）。
      if (snapshot.config.config.update.autoCheck) {
        setTimeout(() => {
          void get().checkUpdate(false);
        }, 3000);
      }
    } catch (error) {
      set({ loading: false, ready: true });
      get().pushToast("error", "初始化失败", messageOf(error));
    }
  },

  t: (key, vars) => translate(get().language, key, vars),

  refresh: async () => {
    try {
      const snapshot = await api.getSnapshot();
      set({
        snapshot,
        config: snapshot.config.config,
        envelope: snapshot.config,
        engine: snapshot.engine,
        vpn: snapshot.vpn,
        campus: snapshot.campus ?? get().campus,
        subscription: snapshot.subscription,
        system: snapshot.system,
        core: snapshot.core,
        presets: snapshot.presets,
        logs: snapshot.logs,
      });
    } catch (error) {
      get().pushToast("error", "刷新失败", messageOf(error));
    }
  },

  loadProxyGroups: async () => {
    try {
      const groups = await api.getProxyGroups();
      set({ proxyGroups: groups });
    } catch {
      set({ proxyGroups: [] });
    }
  },

  checkUpdate: async (force = true) => {
    set({ updateChecking: true });
    try {
      const info = await api.checkUpdate();
      const skipped = get().config?.update?.skippedVersion ?? null;
      set({ updateChecking: false, update: info });
      if (force && info.hasUpdate && info.latest !== skipped) {
        set({ updateDialogOpen: true });
      }
      return info;
    } catch (error) {
      set({ updateChecking: false });
      if (force) {
        get().pushToast("error", get().t("update.checkFailed"), messageOf(error));
      }
      return null;
    }
  },

  skipUpdate: async (version) => {
    try {
      const envelope = await api.skipUpdateVersion(version);
      set({ envelope, config: envelope.config, updateDialogOpen: false });
      get().pushToast(
        "info",
        version ? get().t("update.skippedToast", { version }) : get().t("update.resumedToast"),
      );
    } catch (error) {
      get().pushToast("error", get().t("update.checkFailed"), messageOf(error));
    }
  },

  openUpdateDialog: () => set({ updateDialogOpen: true }),
  closeUpdateDialog: () => set({ updateDialogOpen: false }),

  saveConfig: async (config, secrets = {}) => {
    set({ busy: "save" });
    try {
      const envelope = await api.saveConfig(config, secrets);
      set({
        envelope,
        config: envelope.config,
        themeMode: envelope.config.ui.theme,
        language: envelope.config.ui.language,
        busy: null,
      });
      get().pushToast("success", get().t("toast.saved"));
      return true;
    } catch (error) {
      set({ busy: null });
      get().pushToast("error", "保存失败", messageOf(error));
      return false;
    }
  },

  applyConfig: async () => {
    set({ busy: "apply" });
    try {
      const outcome = await api.applyConfig();
      if (outcome) {
        set({ engine: outcome.engine, vpn: outcome.vpn, campus: outcome.campus ?? get().campus });
      }
      set({ busy: null });
      get().pushToast("success", get().t("toast.applied"));
      if (outcome?.warnings.length) {
        get().pushToast("warn", get().t("common.warning"), outcome.warnings[0]);
      }
      return true;
    } catch (error) {
      set({ busy: null });
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
      return false;
    }
  },

  connect: async () => {
    set({ busy: "connect" });
    try {
      const outcome = await api.startEngine();
      set({
        engine: outcome.engine,
        vpn: outcome.vpn,
        campus: outcome.campus ?? get().campus,
        busy: null,
      });
      get().pushToast("success", get().t("toast.connected"));
      if (outcome.warnings.length > 0) {
        get().pushToast("warn", get().t("common.warning"), outcome.warnings[0]);
      }
      void get().loadProxyGroups();
      void get().loadLogs();
    } catch (error) {
      set({ busy: null });
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
    }
  },

  disconnect: async () => {
    set({ busy: "disconnect" });
    try {
      const engine = await api.stopEngine();
      set({ engine, proxyGroups: [], busy: null });
      get().pushToast("info", get().t("toast.disconnected"));
      void get().loadLogs();
    } catch (error) {
      set({ busy: null });
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
    }
  },

  restart: async () => {
    set({ busy: "restart" });
    try {
      const outcome = await api.restartEngine();
      set({ engine: outcome.engine, vpn: outcome.vpn, busy: null });
      get().pushToast("success", get().t("toast.connected"));
      void get().loadProxyGroups();
    } catch (error) {
      set({ busy: null });
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
    }
  },

  refreshCampus: async () => {
    try {
      const campus = await api.refreshCampus();
      set({ campus });
      get().pushToast("info", get().t("toast.campusRefreshed"));
    } catch (error) {
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
    }
  },

  setCampusOverride: async (value) => {
    try {
      const campus = await api.setCampusOverride(value);
      set({ campus });
    } catch (error) {
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
    }
  },

  importSubscription: async (url, name) => {
    set({ busy: "subscription" });
    try {
      const subscription = await api.importSubscription(url, name);
      set({ subscription, busy: null });
      get().pushToast("success", get().t("toast.importOk"));
      await get().refresh();
      return true;
    } catch (error) {
      set({ busy: null });
      get().pushToast("error", get().t("toast.importOk") + "失败", messageOf(error));
      return false;
    }
  },

  testSubscription: async (url) => {
    set({ busy: "subscription-test" });
    try {
      const info = await api.testSubscription(url);
      set({ busy: null });
      get().pushToast("success", get().t("toast.testOk"), `${info.proxyCount} 个节点`);
      return info;
    } catch (error) {
      set({ busy: null });
      get().pushToast("error", get().t("toast.testFailed"), messageOf(error));
      return null;
    }
  },

  refreshSubscription: async () => {
    set({ busy: "subscription" });
    try {
      const subscription = await api.refreshSubscription();
      set({ subscription, busy: null });
      get().pushToast("success", get().t("toast.subscriptionRefreshed"));
    } catch (error) {
      set({ busy: null });
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
    }
  },

  testVpn: async () => {
    set({ busy: "vpn-test" });
    try {
      const result = await api.testVpnConnection();
      set({ busy: null });
      get().pushToast(
        result.ok ? "success" : "error",
        result.ok ? get().t("toast.testOk") : get().t("toast.testFailed"),
        result.message,
      );
      return result;
    } catch (error) {
      set({ busy: null });
      get().pushToast("error", get().t("toast.testFailed"), messageOf(error));
      return null;
    }
  },

  testSsh: async (host, port) => {
    set({ busy: "ssh-test" });
    try {
      const result = await api.testSsh(host, port);
      set({ busy: null });
      get().pushToast(result.ok ? "success" : "error", result.ok ? get().t("toast.testOk") : get().t("toast.testFailed"), result.message);
      return result;
    } catch (error) {
      set({ busy: null });
      get().pushToast("error", get().t("toast.testFailed"), messageOf(error));
      return null;
    }
  },

  loadAnalytics: async (range) => {
    const nextRange = range ?? get().analyticsRange;
    try {
      const data = await api.getAnalytics(nextRange);
      set({ analytics: data, analyticsRange: nextRange });
    } catch (error) {
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
    }
  },

  loadAiExposure: async (range) => {
    const nextRange = range ?? get().analyticsRange;
    try {
      const data = await api.getAiExposure(nextRange);
      set({ aiExposure: data });
    } catch (error) {
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
    }
  },

  updateAiDomains: async (domains) => {
    try {
      const normalized = await api.setAiDomains(domains);
      const analytics = get().analytics;
      set({ analytics: analytics ? { ...analytics, aiDomains: normalized } : analytics });
      get().pushToast("success", get().t("analytics.aiSaved"), `${normalized.length}`);
      return true;
    } catch (error) {
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
      return false;
    }
  },

  loadNodeCandidates: async () => {
    try {
      const candidates = await api.getNodeCandidates();
      set({ nodeCandidates: candidates });
    } catch (error) {
      set({ nodeCandidates: [] });
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
    }
  },

  chooseNode: async (name) => {
    try {
      await api.selectNode(name);
      await get().loadNodeCandidates();
      await get().refresh();
      get().pushToast("success", get().t("nodeSelection.switched"), name);
      return true;
    } catch (error) {
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
      return false;
    }
  },

  selectProxy: async (group, name) => {
    try {
      await api.selectProxy(group, name);
      await get().loadProxyGroups();
    } catch (error) {
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
    }
  },

  testProxyDelay: async () => {
    set({ busy: "delay" });
    try {
      const groups = get().proxyGroups;
      const auto = groups.find((group) => group.kind === "URLTest") ?? groups[0];
      if (auto) await api.testProxyDelay(auto.name);
      await get().loadProxyGroups();
    } catch (error) {
      get().pushToast("error", get().t("toast.testFailed"), messageOf(error));
    } finally {
      set({ busy: null });
    }
  },

  detectVpnBinary: async () => {
    try {
      return await api.detectVpnBinary();
    } catch (error) {
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
      return null;
    }
  },

  installHelper: async () => {
    set({ busy: "helper" });
    try {
      const result = await api.installHelper();
      set({ busy: null });
      get().pushToast("success", get().t("toast.helperInstalled"), result.message);
      await get().refresh();
    } catch (error) {
      set({ busy: null });
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
    }
  },

  uninstallHelper: async () => {
    set({ busy: "helper" });
    try {
      const result = await api.uninstallHelper();
      set({ busy: null });
      get().pushToast("info", result.message);
      await get().refresh();
    } catch (error) {
      set({ busy: null });
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
    }
  },

  startHelperElevated: async () => {
    set({ busy: "helper" });
    try {
      const result = await api.startHelperElevated();
      set({ busy: null });
      get().pushToast("info", result.message);
      await get().refresh();
    } catch (error) {
      set({ busy: null });
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
    }
  },

  setSystemProxy: async (enabled) => {
    try {
      const result = await api.setSystemProxy(enabled);
      get().pushToast("info", result.message);
      await get().refresh();
    } catch (error) {
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
    }
  },

  setAutostart: async (enabled) => {
    try {
      const result = await api.setAutostart(enabled);
      get().pushToast("info", result.message);
      await get().refresh();
    } catch (error) {
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
    }
  },

  setThemeMode: (mode) => {
    const theme = resolveTheme(mode);
    applyThemeClass(theme);
    set({ themeMode: mode, resolvedTheme: theme });
    const config = get().config;
    if (config) {
      const next = { ...config, ui: { ...config.ui, theme: mode } };
      set({ config: next });
    }
  },

  setLanguage: (language) => {
    set({ language });
    const config = get().config;
    if (config) set({ config: { ...config, ui: { ...config.ui, language } } });
  },

  pushToast: (kind, title, message) => {
    const item: ToastItem = { id: newId(), kind, title, message };
    set((state) => ({ toasts: [...state.toasts, item].slice(-4) }));
    setTimeout(() => {
      set((state) => ({ toasts: state.toasts.filter((toast) => toast.id !== item.id) }));
    }, 4200);
  },

  dismissToast: (id) => {
    set((state) => ({ toasts: state.toasts.filter((toast) => toast.id !== id) }));
  },

  loadLogs: async (limit = 200) => {
    try {
      const logs = await api.getLogs(limit);
      set({ logs });
    } catch {
      // 日志读取失败不影响主流程
    }
  },

  clearLogs: async () => {
    try {
      await api.clearLogs();
      set({ logs: [] });
    } catch (error) {
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
    }
  },

  loadPreview: async () => {
    try {
      const preview = await api.previewConfig();
      set({ preview });
    } catch (error) {
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
    }
  },

  diagnose: async (input) => {
    set({ busy: "diagnose" });
    try {
      const result = await api.diagnoseTarget(input);
      set({ busy: null });
      return result;
    } catch (error) {
      set({ busy: null });
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
      return null;
    }
  },

  validateRules: async (rules) => {
    try {
      const result = await api.validateRules(rules);
      if (result.invalid.length > 0) {
        get().pushToast("error", get().t("toast.rulesInvalid"), result.warnings[0]);
      } else {
        get().pushToast("success", get().t("toast.rulesValid"));
      }
      return result;
    } catch (error) {
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
      return null;
    }
  },

  exportDiagnostics: async () => {
    try {
      const result = await api.exportDiagnostics();
      get().pushToast("success", get().t("toast.diagnosticsExported"), result.detail ?? undefined);
    } catch (error) {
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
    }
  },

  openPath: async (target) => {
    try {
      await api.openPath(target);
    } catch (error) {
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
    }
  },

  exportConfig: async () => {
    try {
      return await api.exportConfig();
    } catch (error) {
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
      return null;
    }
  },

  resetConfig: async () => {
    set({ busy: "reset" });
    try {
      const envelope = await api.resetConfig();
      set({ envelope, config: envelope.config, busy: null });
      get().pushToast("info", get().t("settings.reset"));
      return true;
    } catch (error) {
      set({ busy: null });
      get().pushToast("error", get().t("toast.failed"), messageOf(error));
      return false;
    }
  },
}));

/** 测试/调试用：停止 mock 定时器。 */
export function stopMockTraffic(): void {
  if (mockTrafficTimer) {
    clearInterval(mockTrafficTimer);
    mockTrafficTimer = null;
  }
}
