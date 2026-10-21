/** 前后端同源常量（代理组名/路由/密钥键）。 */

export const GROUP = {
  proxy: "🚀 代理",
  auto: "♻️ 自动选择",
  campus: "🎓 校内",
  ssh: "🐧 SSH 转发",
  adblock: "🛑 广告拦截",
  final: "🐟 漏网之鱼",
  fdu: "🏫 校内 VPN",
} as const;

export const ROUTES = {
  dashboard: "/",
  analytics: "/analytics",
  setupSubscription: "/setup/subscription",
  setupCampus: "/setup/campus",
  setupSsh: "/setup/ssh",
  merge: "/merge",
  logs: "/logs",
  settings: "/settings",
} as const;

export type RoutePath = (typeof ROUTES)[keyof typeof ROUTES];

/** 密钥库键名（与 Rust 侧 secret_keys 保持一致）。 */
export const SECRET_KEYS = {
  campusPassword: "campus_vpn.password",
  campusTotp: "campus_vpn.totp_secret",
  sshPassword: "ssh.password",
  sshKeyPassphrase: "ssh.key_passphrase",
  controllerSecret: "engine.controller_secret",
} as const;

/** 事件名（与 Rust events.rs 保持一致）。 */
export const EVENTS = {
  engineStatus: "engine://status",
  vpnStatus: "vpn://status",
  campusStatus: "campus://status",
  traffic: "traffic://update",
  navigateSettings: "navigate://settings",
} as const;

export const APP_VERSION = "0.1.6";
export const MAX_TRAFFIC_POINTS = 60;
