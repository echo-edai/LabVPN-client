#!/usr/bin/env node
/**
 * 下载当前平台所需的 sidecar 二进制到 apps/desktop/src-tauri/binaries/。
 *
 * 用法：
 *   node scripts/fetch-sidecars.mjs            # 下载 mihomo（当前平台）
 *   node scripts/fetch-sidecars.mjs --force    # 强制重新下载
 *   node scripts/fetch-sidecars.mjs --helper   # 额外编译并复制 labvpn-helper
 *
 * 说明：
 * - mihomo 为 GPL-3.0，这里只下载官方 release，不修改、不重新分发。
 * - fdu-connect（AGPL-3.0）不在此脚本内：默认由 App 发现用户已安装的版本，
 *   或按 docs/BUILD.md 自行编译。
 */
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { chmodSync, copyFileSync, existsSync, mkdirSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const BIN_DIR = join(ROOT, "apps", "desktop", "src-tauri", "binaries");

/** 固定版本，保证可复现；升级时同步更新 CHANGELOG */
const MIHOMO_VERSION = "v1.19.32";
const WINTUN_VERSION = "0.14.1";
const RELEASE_BASE = `https://github.com/MetaCubeX/mihomo/releases/download/${MIHOMO_VERSION}`;

const args = process.argv.slice(2);
const force = args.includes("--force");
const withHelper = args.includes("--helper");

function log(message) {
  console.log(`[sidecars] ${message}`);
}

function fail(message) {
  console.error(`[sidecars] ❌ ${message}`);
  process.exit(1);
}

function platformAsset() {
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

async function download(url, destination) {
  log(`下载 ${url}`);
  const response = await fetch(url, { redirect: "follow" });
  if (!response.ok) fail(`下载失败 HTTP ${response.status}: ${url}`);
  const buffer = Buffer.from(await response.arrayBuffer());
  writeFileSync(destination, buffer);
  const digest = createHash("sha256").update(buffer).digest("hex");
  log(`已保存 ${destination}（${(buffer.length / 1024 / 1024).toFixed(1)} MB, sha256=${digest.slice(0, 16)}…）`);
  return buffer;
}

function exec(command, commandArgs, options = {}) {
  return execFileSync(command, commandArgs, { stdio: "inherit", ...options });
}

/**
 * 在目录中递归查找匹配后缀的文件（mihomo/wintun 压缩包内部命名与层级可能变化）。
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
    const preferred = matches.find((file) => file.toLowerCase().split(/[\\/]/).includes(preferSegment));
    if (preferred) return preferred;
  }
  return matches.sort()[0];
}

async function fetchMihomo() {
  const target = join(BIN_DIR, process.platform === "win32" ? "mihomo.exe" : "mihomo");
  const resourceName = "mihomo";
  const finalPath = join(BIN_DIR, resourceName);
  if (existsSync(finalPath) && !force) {
    log(`已存在 ${finalPath}，跳过（--force 可强制更新）`);
    return finalPath;
  }
  mkdirSync(BIN_DIR, { recursive: true });
  const asset = platformAsset();
  const archive = join(tmpdir(), asset.name);
  await download(`${RELEASE_BASE}/${asset.name}`, archive);

  if (asset.kind === "gz") {
    exec("gunzip", ["-f", archive]);
    const extracted = archive.replace(/\.gz$/, "");
    copyFileSync(extracted, finalPath);
    chmodSync(finalPath, 0o755);
    rmSync(extracted, { force: true });
  } else {
    // zip：Windows 用 PowerShell 解压，其他平台用 unzip（脚本通常只在 Windows 走这里）
    const outDir = join(tmpdir(), `mihomo-${Date.now()}`);
    mkdirSync(outDir, { recursive: true });
    if (process.platform === "win32") {
      exec("powershell", ["-NoProfile", "-Command", `Expand-Archive -Force -Path '${archive}' -DestinationPath '${outDir}'`]);
    } else {
      exec("unzip", ["-o", archive, "-d", outDir]);
    }
    const extractedBinary = findFile(outDir, ".exe");
    if (!extractedBinary) fail("mihomo 压缩包中没有找到 .exe 文件");
    copyFileSync(extractedBinary, finalPath);
    rmSync(outDir, { recursive: true, force: true });
    rmSync(archive, { force: true });
  }
  log(`✅ mihomo 就绪：${finalPath}（${(statSync(finalPath).size / 1024 / 1024).toFixed(1)} MB）`);
  return finalPath;
}

async function fetchWintun() {
  if (process.platform !== "win32") return;
  const target = join(BIN_DIR, "wintun.dll");
  if (existsSync(target) && !force) {
    log(`已存在 ${target}，跳过`);
    return;
  }
  const url = `https://www.wintun.net/builds/wintun-${WINTUN_VERSION}.zip`;
  const archive = join(tmpdir(), `wintun-${WINTUN_VERSION}.zip`);
  await download(url, archive);
  const outDir = join(tmpdir(), `wintun-${Date.now()}`);
  mkdirSync(outDir, { recursive: true });
  exec("powershell", ["-NoProfile", "-Command", `Expand-Archive -Force -Path '${archive}' -DestinationPath '${outDir}'`]);
  const extractedDll = findFile(outDir, "wintun.dll");
  if (!extractedDll) fail("wintun 压缩包中没有找到 wintun.dll");
  copyFileSync(extractedDll, target);
  rmSync(outDir, { recursive: true, force: true });
  rmSync(archive, { force: true });
  log(`✅ wintun.dll 就绪：${target}`);
}

function buildHelper() {
  const helperName = process.platform === "win32" ? "labvpn-helper.exe" : "labvpn-helper";
  const release = join(ROOT, "target", "release", helperName);
  log("编译 labvpn-helper（release）…");
  exec("cargo", ["build", "-p", "labvpn-helper", "--release"], { cwd: ROOT });
  if (!existsSync(release)) fail(`找不到编译产物：${release}`);
  mkdirSync(BIN_DIR, { recursive: true });
  copyFileSync(release, join(BIN_DIR, "labvpn-helper"));
  chmodSync(join(BIN_DIR, "labvpn-helper"), 0o755);
  log(`✅ labvpn-helper 就绪：${join(BIN_DIR, "labvpn-helper")}`);
}

async function main() {
  log(`平台：${process.platform}/${process.arch}，mihomo 版本：${MIHOMO_VERSION}`);
  await fetchMihomo();
  await fetchWintun();
  if (withHelper) buildHelper();
  // 生成清单，便于 CI 与排障
  const manifest = {
    mihomo: MIHOMO_VERSION,
    wintun: process.platform === "win32" ? WINTUN_VERSION : null,
    platform: process.platform,
    arch: process.arch,
    fetchedAt: new Date().toISOString(),
  };
  writeFileSync(join(BIN_DIR, "manifest.json"), JSON.stringify(manifest, null, 2) + "\n");
  log("完成 ✅");
}

main().catch((error) => fail(error?.stack ?? String(error)));
