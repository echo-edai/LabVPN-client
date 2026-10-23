#!/usr/bin/env node
/**
 * 准备 sidecar 二进制到 apps/desktop/src-tauri/binaries/。
 *
 * 用法：
 *   node scripts/fetch-sidecars.mjs                 # mihomo + fdu-connect（自动查找/编译）
 *   node scripts/fetch-sidecars.mjs --force         # 强制重新下载 mihomo
 *   node scripts/fetch-sidecars.mjs --helper        # 额外编译 labvpn-helper
 *   node scripts/fetch-sidecars.mjs --require-fdu   # fdu-connect 缺失时失败（Release 用）
 *
 * 产物（每个都同时存在带/不带 .exe 两个文件名，满足跨平台资源映射）：
 *   binaries/mihomo(.exe)           GPL-3.0，官方 Release
 *   binaries/fdu-connect(.exe)      AGPL-3.0，校内 RVPN（打包，附许可证与源码归档）
 *   binaries/labvpn-helper(.exe)    本仓库特权 helper（--helper）
 *   binaries/wintun.dll             Windows TUN 依赖
 *
 * 安全/合规要点：
 * - fdu-connect 以独立进程使用；**源码经本地补丁修改**（见 vendor/fdu-connect/PATCH.md），
 *   完整修改后源码随仓库提供（AGPL-3.0），许可证与 NOTICE 随 App 打包；
 *   本脚本优先用 vendor/ 下的源码编译，并记录基线 commit 与补丁信息。
 * - 脚本不写入任何用户凭据。
 */
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { chmodSync, copyFileSync, existsSync, mkdirSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const BIN_DIR = join(ROOT, "apps", "desktop", "src-tauri", "binaries");

/** mihomo / wintun 版本固定，保证可复现 */
const MIHOMO_VERSION = "v1.19.32";
const WINTUN_VERSION = "0.14.1";
const RELEASE_BASE = `https://github.com/MetaCubeX/mihomo/releases/download/${MIHOMO_VERSION}`;

/** fdu-connect 构建基准（AGPL-3.0） */
const FDU_REPO = "AkiraSalvare/fdu-connect";
const FDU_COMMIT = "a904877f0eda0c7b1d7933ad6e80d5e8d3ffd584";
/** 本地补丁版源码（优先编译；见 vendor/fdu-connect/PATCH.md） */
const FDU_VENDOR_DIR = join(ROOT, "vendor", "fdu-connect");
const FDU_PATCH = "robust-socks.patch";

const args = process.argv.slice(2);
const force = args.includes("--force");
const withHelper = args.includes("--helper");
const requireFdu = args.includes("--require-fdu");

const isWindows = process.platform === "win32";

function log(message) {
  console.log(`[sidecars] ${message}`);
}

function fail(message) {
  console.error(`[sidecars] ❌ ${message}`);
  process.exit(1);
}

function exec(command, commandArgs, options = {}) {
  return execFileSync(command, commandArgs, { stdio: "inherit", ...options });
}

function commandExists(command) {
  try {
    execFileSync(isWindows ? "where" : "which", [command], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

function sha256(buffer) {
  return createHash("sha256").update(buffer).digest("hex");
}

/**
 * 递归查找匹配后缀的文件（压缩包内部命名与层级可能变化）。
 * preferSegment 用于优先匹配路径片段（例如 wintun 的 `amd64`）。
 */
function findFile(root, suffix, preferSegment) {
  const matches = [];
  const stack = [root];
  while (stack.length > 0) {
    const dir = stack.pop();
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = join(dir, entry.name);
      if (entry.isDirectory()) {
        stack.push(full);
      } else if (entry.name.toLowerCase().endsWith(suffix)) {
        matches.push(full);
      }
    }
  }
  if (matches.length === 0) return null;
  if (preferSegment) {
    const preferred = matches.find((file) =>
      file.toLowerCase().split(/[\\/]/).includes(preferSegment),
    );
    if (preferred) return preferred;
  }
  return matches.sort()[0];
}

/**
 * 写入某个 sidecar：真实文件使用平台真名，另一个名字写空占位。
 * 这样 tauri.conf.json 的资源映射（同时包含两种名字）在任意平台都能工作。
 */
function installSidecar(baseName, sourcePath, { placeholder = false } = {}) {
  mkdirSync(BIN_DIR, { recursive: true });
  const realName = isWindows ? `${baseName}.exe` : baseName;
  const otherName = isWindows ? baseName : `${baseName}.exe`;
  if (placeholder) {
    writeFileSync(join(BIN_DIR, realName), "");
    writeFileSync(join(BIN_DIR, otherName), "");
    return;
  }
  copyFileSync(sourcePath, join(BIN_DIR, realName));
  if (!isWindows) chmodSync(join(BIN_DIR, realName), 0o755);
  writeFileSync(join(BIN_DIR, otherName), "");
  log(
    `✅ ${realName}（${(statSync(join(BIN_DIR, realName)).size / 1024 / 1024).toFixed(1)} MB）`,
  );
}

async function download(url, destination, { required = true } = {}) {
  log(`下载 ${url}`);
  const response = await fetch(url, { redirect: "follow" });
  if (!response.ok) {
    const message = `下载失败 HTTP ${response.status}: ${url}`;
    if (required) fail(message);
    throw new Error(message);
  }
  const buffer = Buffer.from(await response.arrayBuffer());
  writeFileSync(destination, buffer);
  log(
    `已保存（${(buffer.length / 1024 / 1024).toFixed(1)} MB, sha256=${sha256(buffer).slice(0, 16)}…）`,
  );
  return buffer;
}

function platformMihomoAsset() {
  const platform = process.platform;
  const arch = process.arch;
  if (platform === "darwin") {
    if (arch === "arm64") return { name: `mihomo-darwin-arm64-${MIHOMO_VERSION}.gz`, kind: "gz" };
    if (arch === "x64") return { name: `mihomo-darwin-amd64-${MIHOMO_VERSION}.gz`, kind: "gz" };
  }
  if (platform === "linux") {
    if (arch === "x64") return { name: `mihomo-linux-amd64-${MIHOMO_VERSION}.gz`, kind: "gz" };
    if (arch === "arm64") return { name: `mihomo-linux-arm64-${MIHOMO_VERSION}.gz`, kind: "gz" };
  }
  if (platform === "win32") {
    if (arch === "x64") return { name: `mihomo-windows-amd64-${MIHOMO_VERSION}.zip`, kind: "zip" };
    if (arch === "arm64") return { name: `mihomo-windows-arm64-${MIHOMO_VERSION}.zip`, kind: "zip" };
  }
  fail(`不支持的平台组合：${platform}/${arch}`);
}

async function fetchMihomo() {
  const realPath = join(BIN_DIR, isWindows ? "mihomo.exe" : "mihomo");
  if (existsSync(realPath) && statSync(realPath).size > 0 && !force) {
    log("mihomo 已存在，跳过（--force 可强制更新）");
    return;
  }
  mkdirSync(BIN_DIR, { recursive: true });
  const asset = platformMihomoAsset();
  const archive = join(tmpdir(), asset.name);
  await download(`${RELEASE_BASE}/${asset.name}`, archive);

  if (asset.kind === "gz") {
    exec("gunzip", ["-f", archive]);
    const extracted = archive.replace(/\.gz$/, "");
    installSidecar("mihomo", extracted);
    rmSync(extracted, { force: true });
  } else {
    const outDir = join(tmpdir(), `mihomo-${Date.now()}`);
    mkdirSync(outDir, { recursive: true });
    if (isWindows) {
      exec("powershell", [
        "-NoProfile",
        "-Command",
        `Expand-Archive -Force -Path '${archive}' -DestinationPath '${outDir}'`,
      ]);
    } else {
      exec("unzip", ["-o", archive, "-d", outDir]);
    }
    const binary = findFile(outDir, ".exe");
    if (!binary) fail("mihomo 压缩包中没有找到 .exe 文件");
    installSidecar("mihomo", binary);
    rmSync(outDir, { recursive: true, force: true });
    rmSync(archive, { force: true });
  }
}

/** 内置 geodata：mihomo 首启无需联网下载 GeoSite/MMDB（避免 20s+ 启动超时）。 */
async function fetchGeoData() {
  const GEO_BASE = "https://github.com/MetaCubeX/meta-rules-dat/releases/download/latest";
  const assets = [
    { name: "GeoSite.dat", url: `${GEO_BASE}/GeoSite.dat` },
    { name: "geoip.metadb", url: `${GEO_BASE}/geoip.metadb` },
  ];
  for (const asset of assets) {
    const target = join(BIN_DIR, asset.name);
    if (existsSync(target) && statSync(target).size > 0 && !force) {
      log(`${asset.name} 已存在，跳过`);
      continue;
    }
    try {
      await download(asset.url, target, { required: false });
    } catch (error) {
      log(`⚠️ 下载 ${asset.name} 失败（离线？）：${error && error.message ? error.message : error}`);
      log("   mihomo 将退回首次启动时自动下载（可能较慢）");
    }
  }
}

async function fetchWintun() {
  if (!isWindows) {
    writeFileSync(join(BIN_DIR, "wintun.dll"), "");
    return;
  }
  const target = join(BIN_DIR, "wintun.dll");
  if (existsSync(target) && statSync(target).size > 0 && !force) {
    log("wintun.dll 已存在，跳过");
    return;
  }
  const archive = join(tmpdir(), `wintun-${WINTUN_VERSION}.zip`);
  await download(`https://www.wintun.net/builds/wintun-${WINTUN_VERSION}.zip`, archive);
  const outDir = join(tmpdir(), `wintun-${Date.now()}`);
  mkdirSync(outDir, { recursive: true });
  exec("powershell", [
    "-NoProfile",
    "-Command",
    `Expand-Archive -Force -Path '${archive}' -DestinationPath '${outDir}'`,
  ]);
  const dll = findFile(outDir, "wintun.dll", "amd64");
  if (!dll) fail("wintun 压缩包中没有找到 amd64/wintun.dll");
  copyFileSync(dll, target);
  rmSync(outDir, { recursive: true, force: true });
  rmSync(archive, { force: true });
  log("✅ wintun.dll 就绪");
}

/** 在常见位置查找用户已安装的 fdu-connect。 */
function findInstalledFduConnect() {
  const names = isWindows ? ["fdu-connect.exe"] : ["fdu-connect"];
  const home = process.env.HOME || process.env.USERPROFILE || "";
  const candidates = [];
  for (const name of names) {
    if (process.env.FDU_CONNECT_BIN) candidates.push(process.env.FDU_CONNECT_BIN);
    candidates.push(join(BIN_DIR, name));
    candidates.push(join(home, "LocalDocuments", "fdu-connect", name));
    candidates.push(join(home, ".local", "bin", name));
    candidates.push("C:\\Program Files\\fdu-connect\\" + name);
    candidates.push("/usr/local/bin/fdu-connect");
    candidates.push("/opt/fdu-connect/fdu-connect");
  }
  for (const candidate of candidates) {
    try {
      if (candidate && existsSync(candidate) && statSync(candidate).size > 0) {
        return candidate;
      }
    } catch {
      // ignore
    }
  }
  return null;
}

/** 编译 vendor/ 下的本地补丁版 fdu-connect（首选；需要 Go 1.24/1.25）。 */
function buildFduConnectFromVendor() {
  const vendorSource = join(FDU_VENDOR_DIR, "main.go");
  if (!existsSync(vendorSource)) {
    return null;
  }
  if (!commandExists("go")) {
    log("未找到 Go 工具链，跳过 vendor 版 fdu-connect 编译");
    return null;
  }
  const output = join(tmpdir(), isWindows ? "fdu-connect-vendor.exe" : "fdu-connect-vendor");
  log(`编译本地补丁版 fdu-connect（基线 ${FDU_COMMIT.slice(0, 12)} + ${FDU_PATCH}）…`);
  try {
    exec("go", ["build", "-trimpath", "-ldflags", "-s -w", "-o", output, "."], {
      cwd: FDU_VENDOR_DIR,
    });
    return output;
  } catch (error) {
    log(`编译 vendor 版失败：${error && error.message ? error.message : error}`);
    return null;
  }
}

/** 从固定 commit 编译上游 fdu-connect（无 Go/patch 源码时的兜底）。 */
function buildFduConnectFromSource() {
  if (!commandExists("go")) {
    return null;
  }
  const workDir = join(tmpdir(), `fdu-connect-${Date.now()}`);
  mkdirSync(workDir, { recursive: true });
  log(`编译 fdu-connect（commit ${FDU_COMMIT.slice(0, 12)}）…`);
  try {
    exec("git", [
      "clone",
      "--filter=blob:none",
      `https://github.com/${FDU_REPO}.git`,
      workDir,
    ]);
    exec("git", ["-C", workDir, "checkout", "--detach", FDU_COMMIT]);
    const output = join(workDir, isWindows ? "fdu-connect.exe" : "fdu-connect");
    exec("go", ["build", "-trimpath", "-ldflags", "-s -w", "-o", output, "."], {
      cwd: workDir,
    });
    return output;
  } catch (error) {
    log(`从源码编译失败：${error && error.message ? error.message : error}`);
    return null;
  }
}

async function fetchFduConnect() {
  const realPath = join(BIN_DIR, isWindows ? "fdu-connect.exe" : "fdu-connect");
  if (existsSync(realPath) && statSync(realPath).size > 0 && !force) {
    log("fdu-connect 已就绪，跳过（--force 可重新准备）");
    return;
  }
  const patched = buildFduConnectFromVendor();
  if (patched) {
    installSidecar("fdu-connect", patched);
    log(`✅ 已编译本地补丁版 fdu-connect（${FDU_PATCH}）`);
    return;
  }
  const installed = findInstalledFduConnect();
  if (installed) {
    log(`⚠️ 使用本机已有 fdu-connect（可能不含补丁）：${installed}`);
    installSidecar("fdu-connect", installed);
    return;
  }
  const built = buildFduConnectFromSource();
  if (built) {
    installSidecar("fdu-connect", built);
    log(`✅ 已从源码编译 fdu-connect（commit ${FDU_COMMIT.slice(0, 12)}）`);
    return;
  }
  if (requireFdu) {
    fail("未找到 fdu-connect：请安装 Go 工具链（CI）或设置 FDU_CONNECT_BIN 指向现有二进制");
  }
  log("⚠️ 未找到 fdu-connect，写入空占位（校内 VPN 功能不可用）");
  installSidecar("fdu-connect", null, { placeholder: true });
}

function buildHelper() {
  const releaseName = isWindows ? "labvpn-helper.exe" : "labvpn-helper";
  const release = join(ROOT, "target", "release", releaseName);
  log("编译 labvpn-helper（release）…");
  exec("cargo", ["build", "-p", "labvpn-helper", "--release"], { cwd: ROOT });
  if (!existsSync(release)) fail(`找不到编译产物：${release}`);
  installSidecar("labvpn-helper", release);
}

function writeManifest() {
  const manifest = {
    mihomo: MIHOMO_VERSION,
    wintun: isWindows ? WINTUN_VERSION : null,
    fduConnect: {
      repo: FDU_REPO,
      commit: FDU_COMMIT,
      license: "AGPL-3.0",
      patched: true,
      patch: FDU_PATCH,
      patchDescription: "cancelable gVisor handshake + SOCKS5 graceful reply, see vendor/fdu-connect/PATCH.md",
    },
    platform: process.platform,
    arch: process.arch,
    fetchedAt: new Date().toISOString(),
  };
  writeFileSync(join(BIN_DIR, "manifest.json"), JSON.stringify(manifest, null, 2) + "\n");
}

async function main() {
  log(`平台：${process.platform}/${process.arch}`);
  mkdirSync(BIN_DIR, { recursive: true });
  await fetchMihomo();
  await fetchWintun();
  await fetchGeoData();
  await fetchFduConnect();
  if (withHelper) buildHelper();
  writeManifest();
  log("完成 ✅");
}

main().catch((error) => fail(error?.stack ?? String(error)));
