/**
 * 浏览器预览模式（非 Tauri）下的后端 Mock。
 *
 * 作用：
 * 1. 让 `npm run dev:web` 可以直接在浏览器里设计与走查 UI；
 * 2. 让截图/自动化测试无需启动 Rust 侧；
 * 3. 前端所有命令都必须在这里有对应实现，保证接口契约清晰。
 */
import { GROUP, SECRET_KEYS } from "@/lib/constants";
import type {
  AiDomainSeries,
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
  LogEntry,
  OperationResult,
  PresetInfo,
  PreviewResult,
  ProxyGroupInfo,
  RuleValidation,
  SshTestResult,
  SubscriptionInfo,
  SystemInfo,
  TrafficPoint,
  VpnStatus,
  VpnTestResult,
  ConnectOutcome,
  UpdateInfo,
  ValidationIssue,
} from "@/lib/types";

const delay = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

const now = () => new Date().toISOString();

function defaultConfig(): AppConfig {
  return {
    version: 1,
    subscription: {
      enabled: true,
      url: "https://sub.example.com/api?token=DEMO-TOKEN",
      name: "我的机场",
      updateIntervalHours: 12,
      userAgent: "clash-verge/v2.4.2",
      useRemoteProvider: true,
      lastUpdated: now(),
      traffic: {
        upload: 8_912_345_678,
        download: 47_231_678_901,
        total: 429_496_729_600,
        expire: Math.floor(Date.now() / 1000) + 86_400 * 62,
      },
      proxyCount: 68,
      fetchMode: "auto",
      fetchProxy: null,
    },
    campusVpn: {
      enabled: true,
      managed: true,
      username: "21300000000",
      passwordRef: SECRET_KEYS.campusPassword,
      totpSecretRef: null,
      server: "stuvpn.fudan.edu.cn",
      port: 443,
      serverIpOverride: "202.120.224.58",
      socksPort: 11080,
      binaryPath: "/Users/lab/LocalDocuments/fdu-connect/fdu-connect",
      extraArgs: [],
      autoDetectCampus: true,
      skipWhenOnCampus: true,
      probeTargets: [
        { name: "校内网关(Ping)", host: "10.134.140.1", port: 0, protocol: "icmp", weight: 100, timeoutMs: 2000 },
        { name: "校内镜像站", host: "10.176.52.2", port: 443, protocol: "tcp", weight: 100, timeoutMs: 1500 },
        { name: "校内 DNS(UDP)", host: "202.120.224.26", port: 53, protocol: "udpDns", weight: 90, timeoutMs: 1500 },
        { name: "校内 DNS(TCP)", host: "202.120.224.26", port: 53, protocol: "tcp", weight: 70, timeoutMs: 1500 },
        { name: "校内应用服务", host: "10.203.8.198", port: 443, protocol: "tcp", weight: 50, timeoutMs: 1500 },
      ],
      campusCidrs: ["10.188.0.0/16", "202.120.0.0/16", "2001:da8:8001::/48"],
      customProxyDomains: ["science.org", "nature.com"],
      proxyAll: true,
      disableFduConfig: false,
      disableFduDns: false,
    },
    sshProxy: {
      enabled: true,
      host: "203.0.113.10",
      port: 22,
      username: "lab",
      auth: "privateKey",
      passwordRef: SECRET_KEYS.sshPassword,
      privateKeyPath: "~/.ssh/id_ed25519",
      privateKeyPassphraseRef: null,
      hostKeys: [],
      routes: [{ value: "git.internal.example.com" }, { value: "10.8.0.0/24" }],
    },
    nodeSelection: {
      mode: "auto",
      selected: null,
      testUrl: "http://www.gstatic.com/generate_204",
      intervalSeconds: 300,
      toleranceMs: 50,
    },
    analytics: {
      enabled: true,
      sampleIntervalSeconds: 15,
      egressIntervalSeconds: 300,
      retentionDays: 90,
      aiDomains: ["chatgpt.com", "openai.com", "claude.ai", "anthropic.com", "gemini.google.com"],
    },
    routing: {
      preset: "smart",
      customRules: ["DOMAIN-SUFFIX,openai.com,🚀 代理", "DOMAIN-SUFFIX,github.com,🚀 代理"],
      customRulesHighPriority: false,
      adblock: true,
      geoRules: true,
      enhancedRules: false,
      dns: {
        fakeIp: true,
        nameservers: ["223.5.5.5", "119.29.29.29"],
        proxyServerNameservers: ["223.5.5.5"],
        campusDns: "202.120.224.26",
        fakeIpFilterExtra: [],
      },
      extraCampusDomains: ["cc98.org"],
    },
    engine: {
      mixedPort: 7897,
      controllerPort: 9097,
      controllerSecretRef: SECRET_KEYS.controllerSecret,
      tun: {
        enabled: true,
        stack: "mixed",
        mtu: 1500,
        dnsHijack: true,
        strictRoute: false,
        routeExcludeExtra: [],
      },
      systemProxy: true,
      allowLan: false,
      ipv6: false,
      logLevel: "info",
      connectOnLaunch: false,
    },
    ui: {
      theme: "dark",
      language: "zh-CN",
      tray: true,
      autoLaunch: false,
      setupCompleted: true,
      setupStep: 3,
      trafficGraph: true,
    },
    update: {
      autoCheck: true,
      skippedVersion: null,
    },
  };
}

let config = defaultConfig();

const campusStatus = (): CampusStatus => ({
  onCampus: true,
  confidence: 0.86,
  checkedAt: now(),
  interface: "en0",
  manualOverride: null,
  signals: [
    { kind: "ipRange", name: "本地 IP 段", ok: true, weight: 80, detail: "10.188.1.10（en0）命中 10.188.0.0/16" },
    { kind: "tcpProbe", name: "TCP 校内镜像站", ok: true, weight: 100, detail: "8ms；已绑定 en0" },
    { kind: "dnsProbe", name: "DNS 校内 DNS(UDP)", ok: true, weight: 90, detail: "14ms（DNS 应答）；已绑定 en0" },
    { kind: "tcpProbe", name: "TCP 校内 DNS(TCP)", ok: true, weight: 70, detail: "12ms；已绑定 en0" },
    { kind: "tcpProbe", name: "TCP 校内应用服务", ok: false, weight: 50, detail: "1500ms（timed out）；已绑定 en0" },
  ],
});

let engine: EngineStatus = {
  state: "running",
  mode: "tun",
  running: true,
  tunEnabled: true,
  helperMode: true,
  mixedPort: 7897,
  controllerPort: 9097,
  pid: 48213,
  startedAt: new Date(Date.now() - 2 * 3600 * 1000).toISOString(),
  lastError: null,
  warnings: [],
};

let vpn: VpnStatus = {
  enabled: true,
  state: "reusing",
  managed: true,
  socksPort: 11080,
  binaryPath: "/Users/lab/LocalDocuments/fdu-connect/fdu-connect",
  pid: 48204,
  message: "端口 11080 已有 fdu-connect 实例，直接复用",
  lastError: null,
  existingInstance: true,
};

let campus = campusStatus();

const presets: PresetInfo[] = [
  { id: "smart", label: "智能分流", description: "校内走 VPN、国内直连、国外走订阅，自动处理 SSH", recommended: true, chinaDirect: true, adblock: false },
  { id: "minimal", label: "极简模式", description: "仅区分校内 / SSH / 订阅三类出口，不做国内直连", recommended: false, chinaDirect: false, adblock: false },
  { id: "strict", label: "严格分流", description: "开启广告拦截，国内域名与 IP 直连，其余全部走订阅", recommended: false, chinaDirect: true, adblock: true },
  { id: "global", label: "全局代理", description: "所有流量都走代理（仍保留防环与局域网直连）", recommended: false, chinaDirect: false, adblock: false },
  { id: "direct", label: "全部直连", description: "全部直连，仅用于排查网络问题", recommended: false, chinaDirect: false, adblock: false },
];

let logs: LogEntry[] = [
  { id: 1, ts: "19:41:02", level: "info", source: "app", message: "LabVPN 启动：配置版本 1，核心 Mihomo Meta v1.19.32（resource）" },
  { id: 2, ts: "19:41:02", level: "info", source: "campus", message: "校园网检测：校内网络（置信度 86%）" },
  { id: 3, ts: "19:41:03", level: "info", source: "vpn", message: "检测到已有 fdu-connect 实例，复用现有 SOCKS 出口" },
  { id: 4, ts: "19:41:04", level: "info", source: "core", message: "已通过特权 helper 启动 TUN 核心" },
  { id: 5, ts: "19:41:06", level: "info", source: "core", message: "核心就绪：Mihomo Meta v1.19.32（控制端口 9097）" },
  { id: 6, ts: "19:41:06", level: "info", source: "orchestrator", message: "连接完成：mode=tun 校内=true vpn=reusing mixed-port=7897" },
  { id: 7, ts: "19:41:07", level: "warn", source: "core", message: "已启用 geo 规则：首次运行需下载 geosite/geoip 数据（约 20MB）" },
  { id: 8, ts: "19:42:11", level: "info", source: "vpn", message: "KeepAlive: OK" },
  { id: 9, ts: "19:45:30", level: "info", source: "subscription", message: "订阅已更新：68 个节点，12 个代理组" },
  { id: 10, ts: "19:46:00", level: "info", source: "engine", message: "切换代理组：🎓 校内 -> DIRECT" },
];

let logId = logs.length;

function pushLog(level: LogEntry["level"], source: string, message: string) {
  logId += 1;
  logs = [...logs.slice(-400), { id: logId, ts: new Date().toTimeString().slice(0, 8), level, source, message }];
}

const proxyGroups = (): ProxyGroupInfo[] => [
  {
    name: GROUP.proxy,
    kind: "Selector",
    now: "♻️ 自动选择",
    all: ["♻️ 自动选择", "🇭🇰 香港 01", "🇯🇵 日本 01", "🇸🇬 新加坡 01", "DIRECT"],
    delay: 46,
  },
  {
    name: GROUP.auto,
    kind: "URLTest",
    now: "🇸🇬 新加坡 01",
    all: ["🇭🇰 香港 01", "🇯🇵 日本 01", "🇸🇬 新加坡 01"],
    delay: 46,
  },
  { name: GROUP.campus, kind: "Selector", now: "DIRECT", all: ["DIRECT", "🏫 校内 VPN"], delay: null },
  { name: GROUP.ssh, kind: "Selector", now: "DIRECT", all: ["DIRECT", "🖥 SSH-203.0.113.10", GROUP.proxy], delay: null },
  { name: GROUP.adblock, kind: "Selector", now: "REJECT", all: ["REJECT", "DIRECT"], delay: null },
  { name: GROUP.final, kind: "Selector", now: GROUP.proxy, all: [GROUP.proxy, "DIRECT"], delay: null },
];

const subscriptionInfo = (): SubscriptionInfo => ({
  enabled: config.subscription.enabled,
  name: config.subscription.name,
  urlMasked: "https://sub.example.com/api?token=***",
  proxyCount: config.subscription.proxyCount ?? 0,
  groupCount: 12,
  lastUpdated: config.subscription.lastUpdated,
  traffic: config.subscription.traffic,
  warnings: [],
  format: "Clash / mihomo 配置",
  proxyNames: ["🇭🇰 香港 01", "🇯🇵 日本 01", "🇸🇬 新加坡 01", "🇺🇸 美国 01", "🇹🇼 台湾 01"],
});

const systemInfo = (): SystemInfo => ({
  os: "macos",
  arch: "aarch64",
  appVersion: "0.1.0",
  helperInstalled: true,
  helperRunning: true,
  helperVersion: "0.1.0",
  defaultInterface: "en0",
  wifiSsid: "fduwireless.1x",
  systemProxyEnabled: false,
  existingVpn: {
    binaryPath: "/Users/lab/LocalDocuments/fdu-connect/fdu-connect",
    socksPort: 11080,
    running: true,
    serviceLabel: "com.fdu-connect",
  },
  portConflicts: [],
  campusOverride: null,
  coreVersion: "Mihomo Meta v1.19.32",
  appUptimeSecs: 1287,
});

const coreInfo = (): CoreInfo => ({
  binaryPath: "/Applications/LabVPN.app/Contents/Resources/bin/mihomo",
  version: "Mihomo Meta v1.19.32",
  source: "resource",
});

const validation = (): ValidationIssue[] => {
  const issues: ValidationIssue[] = [];
  if (config.campusVpn.enabled && !config.campusVpn.username.trim()) {
    issues.push({ field: "campusVpn.username", level: "error", message: "请填写复旦上网账号" });
  }
  if (config.subscription.enabled && !config.subscription.url.trim()) {
    issues.push({ field: "subscription.url", level: "error", message: "已启用订阅，但订阅地址为空" });
  }
  if (config.engine.mixedPort === config.engine.controllerPort) {
    issues.push({ field: "engine.mixedPort", level: "error", message: "混合代理端口不能与控制端口相同" });
  }
  if (config.sshProxy.enabled && config.sshProxy.routes.length === 0) {
    issues.push({ field: "sshProxy.routes", level: "warn", message: "SSH 未绑定任何域名/IP，配置不会生效" });
  }
  return issues;
};

const envelope = (): ConfigEnvelope => ({
  config,
  secretsPresent: {
    [SECRET_KEYS.campusPassword]: true,
    [SECRET_KEYS.sshPassword]: false,
    [SECRET_KEYS.controllerSecret]: true,
  },
  validation: validation(),
  paths: {
    dataDir: "~/Library/Application Support/app.labvpn.desktop",
    configFile: "~/Library/Application Support/app.labvpn.desktop/config.json",
    coreDir: "~/Library/Application Support/app.labvpn.desktop/core",
    helperWorkdir: "~/Library/Application Support/app.labvpn.desktop/helper",
    logsDir: "~/Library/Application Support/app.labvpn.desktop/logs",
    binDir: "~/Library/Application Support/app.labvpn.desktop/bin",
  },
});

const snapshot = (): AppSnapshot => ({
  config: envelope(),
  engine,
  vpn,
  campus,
  subscription: subscriptionInfo(),
  core: coreInfo(),
  system: systemInfo(),
  presets,
  logs: logs.slice(-120),
});

const previewYaml = `mixed-port: 7897
allow-lan: false
mode: rule
log-level: info
external-controller: 127.0.0.1:9097
secret: <secret>
dns:
  enable: true
  enhanced-mode: fake-ip
  fake-ip-range: 198.18.0.1/16
  nameserver: [223.5.5.5, 119.29.29.29]
  nameserver-policy:
    +.fudan.edu.cn: [202.120.224.26]
tun:
  enable: true
  stack: mixed
  auto-route: true
  dns-hijack: [any:53]
  route-exclude-address: [202.120.224.58/32]
proxies:
  - name: 🏫 校内 VPN
    type: socks5
    server: 127.0.0.1
    port: 11080
  - name: 🖥 SSH-203.0.113.10
    type: ssh
    server: 203.0.113.10
    port: 22
    username: lab
proxy-groups:
  - name: 🎓 校内
    type: select
    proxies: [DIRECT, 🏫 校内 VPN]
  - name: 🚀 代理
    type: select
    use: [sub]
rules:
  - DOMAIN,stuvpn.fudan.edu.cn,DIRECT
  - PROCESS-NAME,fdu-connect,DIRECT
  - RULE-SET,FDUIP,🎓 校内
  - RULE-SET,FDUDomain,🎓 校内
  - DOMAIN-SUFFIX,git.internal.example.com,🐧 SSH 转发
  - RULE-SET,AdBlock,🛑 广告拦截
  - GEOSITE,cn,DIRECT
  - GEOIP,CN,DIRECT,no-resolve
  - MATCH,🐟 漏网之鱼`;

const preview = (): PreviewResult => ({
  yaml: previewYaml,
  warnings: [
    "已启用 geo 规则：首次运行需下载 geosite/geoip 数据（约 20MB）",
    "检测到校园网：已跳过 VPN，校内流量直连",
  ],
  stats: { proxyCount: 2, groupCount: 6, providerCount: 5, ruleCount: 28, tunEnabled: true },
  rules: [
    "DOMAIN,stuvpn.fudan.edu.cn,DIRECT",
    "IP-CIDR,202.120.224.58/32,DIRECT,no-resolve",
    "PROCESS-NAME,fdu-connect,DIRECT",
    "PROCESS-NAME,mihomo,DIRECT",
    "IP-CIDR,127.0.0.0/8,DIRECT,no-resolve",
    "RULE-SET,LAN,DIRECT",
    "IP-CIDR,192.168.1.0/24,DIRECT,no-resolve",
    "RULE-SET,FDUIP,🎓 校内",
    "RULE-SET,FDUDomain,🎓 校内",
    "DOMAIN-SUFFIX,cc98.org,🎓 校内",
    "DOMAIN-SUFFIX,git.internal.example.com,🐧 SSH 转发",
    "IP-CIDR,10.8.0.0/24,🐧 SSH 转发,no-resolve",
    "DOMAIN-SUFFIX,openai.com,🚀 代理",
    "DOMAIN-SUFFIX,github.com,🚀 代理",
    "RULE-SET,AdBlock,🛑 广告拦截",
    "GEOSITE,cn,DIRECT",
    "GEOIP,CN,DIRECT,no-resolve",
    "MATCH,🐟 漏网之鱼",
  ],
});

function outcome(ok: boolean): ConnectOutcome {
  return {
    engine,
    vpn,
    campus,
    mode: engine.mode ?? "tun",
    warnings: ok ? [] : ["校内 VPN 启动失败（演示数据）"],
  };
}

function operation(message: string): OperationResult {
  return { ok: true, message, detail: null };
}

const trafficEngine = (): ProxyGroupInfo[] =>
  proxyGroups().filter((group) => group.kind === "Selector");

/** Mock 命令分发。 */
function mockAiExposure(range: string): AiExposureData {
  const now = Math.floor(Date.now() / 1000);
  const hour = 3600;
  const definitions: Record<string, { ip: string; node: string; hoursAgo: number; durationMin: number; host: string }[]> = {
    "chatgpt.com": [
      { ip: "203.0.113.20", node: "HK-01", hoursAgo: 20, durationMin: 240, host: "chatgpt.com" },
      { ip: "203.0.113.22", node: "🇯🇵 日本 02", hoursAgo: 8, durationMin: 90, host: "chatgpt.com" },
      { ip: "203.0.113.20", node: "JP-02", hoursAgo: 3, durationMin: 120, host: "chatgpt.com" },
    ],
    "openai.com": [
      { ip: "203.0.113.31", node: "US-01", hoursAgo: 18, durationMin: 120, host: "api.openai.com" },
      { ip: "198.51.100.7", node: "🇸🇬 新加坡 03", hoursAgo: 6, durationMin: 60, host: "platform.openai.com" },
    ],
    "claude.ai": [
      { ip: "203.0.113.31", node: "🇺🇸 美国 01", hoursAgo: 12, durationMin: 180, host: "claude.ai" },
    ],
    "anthropic.com": [
      { ip: "203.0.113.31", node: "🇺🇸 美国 01", hoursAgo: 11, durationMin: 30, host: "anthropic.com" },
    ],
    "gemini.google.com": [],
  };
  const series: AiDomainSeries[] = Object.entries(definitions).map(([domain, entries]) => {
    const records = entries
      .map((entry) => ({
        ts: now - entry.hoursAgo * hour,
        ip: entry.ip,
        node: entry.node,
        host: entry.host,
      }))
      .sort((a, b) => a.ts - b.ts);
    const segments: AiDomainSeries["segments"] = [];
    for (const record of records) {
      const last = segments[segments.length - 1];
      if (last && last.ip === record.ip && last.node === record.node) {
        last.lastTs = record.ts;
        last.count += 1;
      } else {
        segments.push({
          ip: record.ip,
          node: record.node,
          firstTs: record.ts,
          lastTs: record.ts + (definitions[domain]?.[segments.length]?.durationMin ?? 60) * 60,
          count: 1,
        });
      }
    }
    return { domain, records: [...records].reverse(), segments };
  });
  return {
    range,
    domains: Object.keys(definitions),
    series,
    updatedAt: new Date().toISOString(),
  };
}

function mockAnalytics(range: string): AnalyticsData {
  const now = Math.floor(Date.now() / 1000);
  const hourly = range === "24h";
  const count = hourly ? 24 : range === "7d" ? 7 : 30;
  const step = hourly ? 3600 : 86_400;
  const start = now - (count - 1) * step;
  const bucket = (ts: number, i: number) => {
    const base = 1_800_000 + ((i * 917_311) % 3_400_000);
    const school = { up: Math.round(base * 0.05), down: Math.round(base * 0.18) };
    const overseas = { up: Math.round(base * 0.22), down: Math.round(base * 1.1) };
    const direct = { up: Math.round(base * 0.12), down: Math.round(base * 0.66) };
    const ssh = { up: Math.round(base * 0.01), down: Math.round(base * 0.05) };
    const other = { up: 0, down: 0 };
    const ai = { up: Math.round(overseas.up * 0.2), down: Math.round(overseas.down * 0.25) };
    return { ts, school, overseas, direct, ssh, other, ai };
  };
  const buckets = Array.from({ length: count }, (_, i) => bucket(start + i * step, i));
  const totals = buckets.reduce(
    (acc, item) => {
      for (const key of ["school", "overseas", "direct", "ssh", "other", "ai"] as const) {
        acc[key].up += item[key].up;
        acc[key].down += item[key].down;
      }
      return acc;
    },
    {
      school: { up: 0, down: 0 },
      overseas: { up: 0, down: 0 },
      direct: { up: 0, down: 0 },
      ssh: { up: 0, down: 0 },
      other: { up: 0, down: 0 },
      ai: { up: 0, down: 0 },
    },
  );
  const egress = Array.from({ length: hourly ? 8 : 12 }, (_, i) => {
    const ts = now - (hourly ? 3600 : 86_400) * (hourly ? 3 : 1) * (i + 1) / (hourly ? 1 : 1);
    return {
      ts: Math.floor(ts),
      route: i % 2 === 0 ? "proxy" : "direct",
      ip: i % 2 === 0 ? `203.0.113.${20 + (i % 5)}` : `10.188.1.${100 + (i % 10)}`,
      node: i % 2 === 0 ? (i % 4 === 0 ? "🇭🇰 香港 01" : "🇯🇵 日本 02") : null,
    };
  }).reverse();
  const campus = Array.from({ length: hourly ? 6 : 10 }, (_, i) => ({
    ts: now - (i + 1) * (hourly ? 4 * 3600 : 86_400),
    onCampus: i % 3 !== 0,
  })).reverse();
  return {
    range,
    granularity: hourly ? "hour" : "day",
    totals,
    buckets,
    egress,
    campus,
    aiDomains: ["chatgpt.com", "openai.com", "claude.ai", "anthropic.com", "gemini.google.com"],
    updatedAt: new Date().toISOString(),
  };
}

export async function mockInvoke(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  await delay(120 + Math.random() * 180);
  switch (cmd) {
    case "check_update": {
      // 浏览器预览：固定返回一个可更新版本，便于走查更新弹窗与设置页。
      return {
        current: "0.1.11",
        latest: "0.1.12",
        hasUpdate: true,
        releaseUrl: "https://github.com/echo-edai/LabVPN-client/releases/tag/v0.1.12",
        releaseName: "LabVPN v0.1.12（预览 Mock）",
        notes: "## 浏览器预览 Mock\n\n- 应用内更新检查示例：发现新版本后由用户决定是否更新\n- 支持忽略此版本",
        publishedAt: now(),
        prerelease: true,
      } satisfies UpdateInfo;
    }
    case "skip_update_version": {
      config.update.skippedVersion = String(args?.version ?? "");
      return envelope();
    }
    case "get_snapshot":
      return snapshot();
    case "get_config":
      return envelope();
    case "save_config": {
      const incoming = (args?.config ?? config) as AppConfig;
      config = { ...incoming };
      const secrets = (args?.secrets ?? {}) as Record<string, string>;
      if (secrets[SECRET_KEYS.campusPassword]) {
        pushLog("info", "config", "密钥已更新：campus_vpn.password");
      }
      pushLog("info", "config", "配置已保存");
      return envelope();
    }
    case "save_and_apply":
      return envelope();
    case "reset_config": {
      config = defaultConfig();
      config.ui.setupCompleted = false;
      return envelope();
    }
    case "export_config":
      return JSON.stringify(config, null, 2);
    case "import_config": {
      const json = String(args?.json ?? "{}");
      config = JSON.parse(json) as AppConfig;
      return envelope();
    }
    case "validate_config": {
      config = (args?.config ?? config) as AppConfig;
      return validation();
    }
    case "preview_config":
      return preview();
    case "get_presets":
      return presets;
    case "validate_rules": {
      const rules = (args?.rules ?? []) as string[];
      const valid = rules.filter((rule) => rule.split(",").length >= 3);
      const invalid = rules.filter((rule) => rule.split(",").length < 3);
      return { valid, invalid, warnings: invalid.map((rule) => `无法解析：${rule}`) } satisfies RuleValidation;
    }
    case "diagnose_target": {
      const input = String(args?.input ?? "");
      const lower = input.toLowerCase();
      let matched = "MATCH,🐟 漏网之鱼";
      let outbound: string = GROUP.final;
      const notes: string[] = ["离线模拟：GEOIP/GEOSITE 规则需由内核最终判定"];
      if (lower.includes("fudan.edu.cn") || lower.endsWith(".cn") === false) {
        if (lower.includes("fudan.edu.cn")) {
          matched = "RULE-SET,FDUDomain (DOMAIN-SUFFIX,fudan.edu.cn)";
          outbound = GROUP.campus;
        } else if (lower.startsWith("10.")) {
          matched = "RULE-SET,FDUIP (IP-CIDR,10.0.0.0/8)";
          outbound = GROUP.campus;
        } else if (lower.includes("openai.com") || lower.includes("github.com")) {
          matched = "DOMAIN-SUFFIX," + lower + ",🚀 代理";
          outbound = GROUP.proxy;
        } else if (lower.includes("git.internal")) {
          matched = "DOMAIN-SUFFIX,git.internal.example.com,🐧 SSH 转发";
          outbound = GROUP.ssh;
        }
      }
      return {
        input,
        matched: true,
        rule: matched,
        outbound,
        notes,
      } satisfies DiagnosticsResult;
    }
    case "start_engine": {
      engine = { ...engine, state: "running", running: true, mode: config.engine.tun.enabled ? "tun" : "systemProxy", startedAt: now(), pid: 51234 };
      vpn = { ...vpn, state: "running" };
      pushLog("info", "orchestrator", "连接完成：mode=tun 校内=true");
      return outcome(true);
    }
    case "stop_engine": {
      engine = { ...engine, state: "idle", running: false, mode: null, pid: null, startedAt: null };
      vpn = { ...vpn, state: "stopped" };
      pushLog("info", "orchestrator", "已断开连接");
      return engine;
    }
    case "restart_engine":
      pushLog("info", "orchestrator", "正在重连…");
      return outcome(true);
    case "apply_config":
      pushLog("info", "config", "配置已应用");
      return engine.running ? outcome(true) : null;
    case "engine_status":
      return engine;
    case "get_proxy_groups":
      return engine.running ? trafficEngine() : [];
    case "get_ai_exposure": {
      return mockAiExposure(String(args?.range ?? "24h"));
    }
    case "get_analytics": {
      const range = String(args?.range ?? "24h");
      return mockAnalytics(range);
    }
    case "set_ai_domains": {
      const domains = (args?.domains as string[] | undefined) ?? [];
      pushLog("info", "analytics", `AI 平台域名已更新：${domains.length} 个`);
      return domains;
    }
    case "get_node_candidates":
      return ["🇭🇰 香港 01", "🇯🇵 日本 02", "🇺🇸 美国 03", "🇸🇬 新加坡 04"];
    case "select_node": {
      const name = String(args?.name ?? "");
      pushLog("info", "engine", `代理节点已固定为：${name}`);
      return operation(`已固定节点 ${name}`);
    }
    case "select_proxy": {
      const group = String(args?.group ?? "");
      const name = String(args?.name ?? "");
      pushLog("info", "engine", `切换代理组：${group} -> ${name}`);
      return operation(`已切换到 ${name}`);
    }
    case "test_proxy_delay":
      return 42;
    case "get_campus_status":
    case "refresh_campus": {
      campus = campusStatus();
      return campus;
    }
    case "set_campus_override": {
      const value = args?.value as boolean | null;
      campus = { ...campusStatus(), onCampus: value ?? true, manualOverride: value, confidence: value === null ? 0.86 : 1 };
      return campus;
    }
    case "campus_override":
      return campus.manualOverride;
    case "detect_campus_now":
      return operation("已刷新校园网状态");
    case "import_subscription": {
      const request = (args?.request ?? {}) as { url?: string; name?: string };
      config.subscription.url = request.url ?? config.subscription.url;
      config.subscription.name = request.name || config.subscription.name;
      config.subscription.enabled = true;
      config.subscription.lastUpdated = now();
      config.subscription.proxyCount = 68;
      pushLog("info", "subscription", "订阅已导入：68 个节点");
      return subscriptionInfo();
    }
    case "test_subscription":
      return subscriptionInfo();
    case "refresh_subscription": {
      config.subscription.lastUpdated = now();
      pushLog("info", "subscription", "订阅已刷新：68 个节点");
      return subscriptionInfo();
    }
    case "get_subscription":
      return subscriptionInfo();
    case "detect_vpn_binary":
      return {
        binaryPath: "/Users/lab/LocalDocuments/fdu-connect/fdu-connect",
        socksPort: 1080,
        running: true,
        serviceLabel: "com.fdu-connect",
      } satisfies ExistingVpn;
    case "start_vpn":
      vpn = { ...vpn, state: "running", message: "校内 VPN 已连接" };
      return vpn;
    case "stop_vpn":
      vpn = { ...vpn, state: "stopped", message: null };
      return vpn;
    case "vpn_status":
      return vpn;
    case "test_vpn_connection": {
      await delay(500);
      return {
        ok: true,
        message: "登录成功：账号密码有效，VPN 服务可用",
        logLines: ["Start FDU Connect v0.9.0", "VPN server version: M7.6.8R2", "SOCKS5 server listening on 127.0.0.1:11181", "KeepAlive: OK"],
      } satisfies VpnTestResult;
    }
    case "test_ssh": {
      await delay(400);
      return { ok: true, message: "TCP 连接成功（SSH 端口可达）", elapsedMs: 38 } satisfies SshTestResult;
    }
    case "get_system_info":
      return systemInfo();
    case "install_helper":
      pushLog("info", "helper", "特权 helper 已安装并启动");
      return operation("特权 helper 安装请求已提交（如弹出系统授权请确认）");
    case "uninstall_helper":
      return operation("helper 卸载请求已提交");
    case "start_helper_elevated":
      return operation("已请求以管理员身份启动 helper");
    case "set_system_proxy":
      return operation(args?.enabled ? "系统代理已开启" : "系统代理已关闭");
    case "get_autostart":
      return false;
    case "set_autostart":
      return operation(args?.enabled ? "已开启开机自启" : "已关闭开机自启");
    case "get_logs":
      return logs.slice(-Number(args?.limit ?? 200));
    case "clear_logs":
      logs = [];
      return operation("日志已清空（文件保留）");
    case "open_path":
      return operation("已打开");
    case "export_diagnostics":
      return { ok: true, message: "诊断包已导出", detail: "~/Library/Application Support/app.labvpn.desktop/logs/diagnostics-demo.json" } satisfies OperationResult;
    case "sync_subscription":
      return operation("订阅已更新：68 个节点");
    default:
      throw new Error(`Mock 未实现的命令：${cmd}`);
  }
}

/** Mock 模式下的伪造流量（用于图表展示）。 */
export function mockTrafficPoint(previous?: TrafficPoint): TrafficPoint {
  const up = Math.round(20_000 + Math.random() * 90_000);
  const down = Math.round(180_000 + Math.random() * 1_300_000);
  return {
    up,
    down,
    upTotal: (previous?.upTotal ?? 0) + up,
    downTotal: (previous?.downTotal ?? 0) + down,
    ts: new Date().toTimeString().slice(0, 8),
  };
}
