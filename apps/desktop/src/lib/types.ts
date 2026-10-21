/**
 * 与 Rust 侧 DTO 一一对应的类型定义。
 * 修改时请同步 `apps/desktop/src-tauri/src/dto.rs` 与 `crates/labvpn-core/src/config.rs`。
 */

export type ThemeMode = "system" | "light" | "dark";
export type Language = "zh-CN" | "en-US";

export interface TrafficInfo {
  upload: number;
  download: number;
  total: number;
  expire: number | null;
}

export interface SubscriptionConfig {
  enabled: boolean;
  url: string;
  name: string;
  updateIntervalHours: number;
  userAgent: string;
  useRemoteProvider: boolean;
  lastUpdated: string | null;
  traffic: TrafficInfo | null;
  proxyCount: number | null;
}

export type ProbeProtocol = "tcp" | "udpDns" | "icmp";

export interface ProbeTarget {
  name: string;
  host: string;
  port: number;
  protocol: ProbeProtocol;
  weight: number;
  timeoutMs: number;
}

export interface CampusVpnConfig {
  enabled: boolean;
  managed: boolean;
  username: string;
  passwordRef: string | null;
  totpSecretRef: string | null;
  server: string;
  port: number;
  serverIpOverride: string | null;
  socksPort: number;
  binaryPath: string | null;
  extraArgs: string[];
  autoDetectCampus: boolean;
  skipWhenOnCampus: boolean;
  probeTargets: ProbeTarget[];
  campusCidrs: string[];
  customProxyDomains: string[];
  disableFduConfig: boolean;
  disableFduDns: boolean;
}

export type SshAuthType = "password" | "privateKey";

export interface SshRoute {
  value: string;
}

export interface SshProxyConfig {
  enabled: boolean;
  host: string;
  port: number;
  username: string;
  auth: SshAuthType;
  passwordRef: string | null;
  privateKeyPath: string | null;
  privateKeyPassphraseRef: string | null;
  hostKeys: string[];
  routes: SshRoute[];
}

export type RoutingPreset = "smart" | "minimal" | "strict" | "global" | "direct";

export interface DnsConfig {
  fakeIp: boolean;
  nameservers: string[];
  proxyServerNameservers: string[];
  campusDns: string;
  fakeIpFilterExtra: string[];
}

export interface RoutingConfig {
  preset: RoutingPreset;
  customRules: string[];
  customRulesHighPriority: boolean;
  adblock: boolean;
  geoRules: boolean;
  enhancedRules: boolean;
  dns: DnsConfig;
  extraCampusDomains: string[];
}

export type NodeSelectionMode = "auto" | "manual";

export interface NodeSelectionConfig {
  mode: NodeSelectionMode;
  selected: string | null;
  testUrl: string;
  intervalSeconds: number;
  toleranceMs: number;
}

export type TunStack = "system" | "gvisor" | "mixed";

export interface TunConfig {
  enabled: boolean;
  stack: TunStack;
  mtu: number;
  dnsHijack: boolean;
  strictRoute: boolean;
  routeExcludeExtra: string[];
}

export interface EngineConfig {
  mixedPort: number;
  controllerPort: number;
  controllerSecretRef: string;
  tun: TunConfig;
  systemProxy: boolean;
  allowLan: boolean;
  ipv6: boolean;
  logLevel: string;
  connectOnLaunch: boolean;
}

export interface UiConfig {
  theme: ThemeMode;
  language: Language;
  tray: boolean;
  autoLaunch: boolean;
  setupCompleted: boolean;
  setupStep: number;
  trafficGraph: boolean;
}

export interface AppConfig {
  version: number;
  subscription: SubscriptionConfig;
  campusVpn: CampusVpnConfig;
  sshProxy: SshProxyConfig;
  nodeSelection: NodeSelectionConfig;
  routing: RoutingConfig;
  engine: EngineConfig;
  ui: UiConfig;
}

export interface ValidationIssue {
  field: string;
  level: "error" | "warn";
  message: string;
}

export interface PathsInfo {
  dataDir: string;
  configFile: string;
  coreDir: string;
  helperWorkdir: string;
  logsDir: string;
  binDir: string;
}

export interface ConfigEnvelope {
  config: AppConfig;
  secretsPresent: Record<string, boolean>;
  validation: ValidationIssue[];
  paths: PathsInfo;
}

export type EngineState = "idle" | "starting" | "running" | "stopping" | "error";

export interface EngineStatus {
  state: EngineState;
  mode: "tun" | "systemProxy" | null;
  running: boolean;
  tunEnabled: boolean;
  helperMode: boolean;
  mixedPort: number;
  controllerPort: number;
  pid: number | null;
  startedAt: string | null;
  lastError: string | null;
  warnings: string[];
}

export type VpnState =
  | "disabled"
  | "stopped"
  | "detecting"
  | "starting"
  | "running"
  | "reusing"
  | "error";

export interface VpnStatus {
  enabled: boolean;
  state: VpnState;
  managed: boolean;
  socksPort: number;
  binaryPath: string | null;
  pid: number | null;
  message: string | null;
  lastError: string | null;
  existingInstance: boolean;
}

export type SignalKind = "ipRange" | "tcpProbe" | "dnsProbe" | "pingProbe" | "wifi" | "manual";

export interface CampusSignal {
  kind: SignalKind;
  name: string;
  ok: boolean;
  weight: number;
  detail: string;
}

export interface CampusStatus {
  onCampus: boolean;
  confidence: number;
  signals: CampusSignal[];
  checkedAt: string;
  interface: string | null;
  manualOverride: boolean | null;
}

export interface SubscriptionInfo {
  enabled: boolean;
  name: string;
  urlMasked: string;
  proxyCount: number;
  groupCount: number;
  lastUpdated: string | null;
  traffic: TrafficInfo | null;
  warnings: string[];
  format: string | null;
  proxyNames: string[];
}

export interface CoreInfo {
  binaryPath: string | null;
  version: string | null;
  source: string;
}

export interface ExistingVpn {
  binaryPath: string | null;
  socksPort: number | null;
  running: boolean;
  serviceLabel: string | null;
}

export interface SystemInfo {
  os: string;
  arch: string;
  appVersion: string;
  helperInstalled: boolean;
  helperRunning: boolean;
  helperVersion: string | null;
  defaultInterface: string | null;
  wifiSsid: string | null;
  systemProxyEnabled: boolean;
  existingVpn: ExistingVpn;
  portConflicts: string[];
  campusOverride: boolean | null;
  coreVersion: string | null;
  appUptimeSecs: number;
}

export interface PresetInfo {
  id: RoutingPreset;
  label: string;
  description: string;
  recommended: boolean;
  chinaDirect: boolean;
  adblock: boolean;
}

export interface LogEntry {
  id: number;
  ts: string;
  level: "info" | "warn" | "error" | string;
  source: string;
  message: string;
}

export interface ProxyGroupInfo {
  name: string;
  kind: string;
  now: string | null;
  all: string[];
  delay: number | null;
}

export interface TrafficPoint {
  up: number;
  down: number;
  upTotal: number;
  downTotal: number;
  ts: string;
}

export interface AppSnapshot {
  config: ConfigEnvelope;
  engine: EngineStatus;
  vpn: VpnStatus;
  campus: CampusStatus | null;
  subscription: SubscriptionInfo;
  core: CoreInfo;
  system: SystemInfo;
  presets: PresetInfo[];
  logs: LogEntry[];
}

export interface PreviewResult {
  yaml: string;
  warnings: string[];
  stats: {
    proxyCount: number;
    groupCount: number;
    providerCount: number;
    ruleCount: number;
    tunEnabled: boolean;
  };
  rules: string[];
}

export interface DiagnosticsResult {
  input: string;
  matched: boolean;
  rule: string;
  outbound: string;
  notes: string[];
}

export interface RuleValidation {
  valid: string[];
  invalid: string[];
  warnings: string[];
}

export interface OperationResult {
  ok: boolean;
  message: string;
  detail: string | null;
}

export interface ConnectOutcome {
  engine: EngineStatus;
  vpn: VpnStatus;
  campus: CampusStatus | null;
  mode: string;
  warnings: string[];
}

export interface VpnTestResult {
  ok: boolean;
  message: string;
  logLines: string[];
}

export interface SshTestResult {
  ok: boolean;
  message: string;
  elapsedMs: number;
}

export type SecretUpdates = Record<string, string>;
