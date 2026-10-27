# LabVPN

> Fudan campus network · subscription · SSH — one client with automatic traffic splitting.

[中文](README.md) | English

[![CI](https://github.com/echo-edai/LabVPN-client/actions/workflows/ci.yml/badge.svg)](https://github.com/echo-edai/LabVPN-client/actions/workflows/ci.yml)
[![Release](https://img.shields.io/badge/release-v0.1.12--pre-8b5cf6)](https://github.com/echo-edai/LabVPN-client/releases/tag/v0.1.12)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

LabVPN integrates **Clash/mihomo subscription + Fudan RVPN + SSH outbound** into one desktop app:
three independent config panels, one merged config, tuned routing rules, and a local network dashboard.
Built with Tauri 2 + mihomo (Clash.Meta), for macOS / Windows / Ubuntu / AlmaLinux. **fdu-connect is bundled.**

> **Disclaimer**: This project is for learning, exchange and technical research only.
> It does **not** provide, share or sell any VPN / proxy subscription, account or service.
> You must use your own lawful Fudan campus account and your own subscription, and comply with
> university network policies and local laws. Welcome to fork, open issues and contribute.

## Download

Latest pre-release **v0.1.12** (unsigned; macOS: right-click → Open, Windows: SmartScreen → Run anyway):

| Platform | Artifacts |
|---|---|
| macOS ARM / Intel | `LabVPN_0.1.12_aarch64.dmg` / `LabVPN_0.1.12_x64.dmg` |
| Windows 10/11 | `LabVPN_0.1.12_x64-setup.exe` / `.msi` |
| Ubuntu 22.04+ | `.deb` / `.AppImage` |
| AlmaLinux 9+ | `.rpm` |

👉 [Releases · v0.1.12 (pre-release)](https://github.com/echo-edai/LabVPN-client/releases/tag/v0.1.12)

## Features

- **Three independent configs** (enable any combination, no forced wizard steps):
  subscription / campus VPN (built-in fdu-connect) / SSH forwarding.
- **Automatic split routing**: `10.0.0.0/8` + Fudan domains → campus; China domains/IP → direct;
  `geolocation-!cn` → subscription proxy; custom targets → SSH; fallback `🐟` defaults to DIRECT.
- **Campus auto-detection**: pings a campus gateway (default `10.134.140.1`, editable) — on success it
  skips the VPN and connects to campus resources directly; interface-bound probes bypass third-party TUNs.
  The UI shows “on campus / off campus” plus the evidence, not a vague confidence score.
- **Node selection**: manual pin (hot-switch) or automatic lowest-latency with custom test URL /
  interval / tolerance (mihomo `url-test`).
- **Network dashboard**: local traffic history by category (24h/7d/30d), direct/proxy egress IP,
  campus timeline, and **AI platform IP exposure** (per-domain egress IP + node over time).
  Data is stored only on your machine (`stats/`, 90-day retention by default).
- **TUN or system proxy**: privileged helper for full-traffic TUN, with graceful fallback.
- **Security**: credentials encrypted at rest (AES-256-GCM + machine-bound key, 0600);
  temporary config files containing secrets are overwritten and deleted right after the core starts;
  logs/diagnostics are redacted; nothing is uploaded to the cloud.

## Screenshots

See [docs/screenshots/](docs/screenshots/) (dashboard, config panels, routing/rules, network dashboard).

## Development quick start

Requires Rust stable, Node.js 20+, and platform WebView dependencies.

```bash
cd apps/desktop
npm install
npm run sidecars   # fetch mihomo (+ build/attach fdu-connect)
npm run dev        # Tauri dev (Vite + Rust)

# logic tests
cd ../.. && cargo test -p labvpn-core
```

Docs: [ARCHITECTURE](docs/ARCHITECTURE.md), [RULES](docs/RULES.md), [BUILD](docs/BUILD.md),
[USER GUIDE (zh)](docs/USER_GUIDE.md), [TROUBLESHOOTING](docs/TROUBLESHOOTING.md),
[NOTICE](NOTICE).

## License

[MIT](LICENSE). Bundled sidecars: mihomo (GPL-3.0), fdu-connect (AGPL-3.0) — see [NOTICE](NOTICE);
corresponding source archives are attached to releases.
