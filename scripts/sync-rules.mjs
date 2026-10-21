#!/usr/bin/env node
/**
 * 同步复旦分流规则数据（来自 AkiraSalvare/fdu-netrules）。
 *
 * 用法：node scripts/sync-rules.mjs [--check]
 *   --check  仅检查是否有更新，不写文件（用于 CI）
 *
 * 输出：
 *   resources/rules/fdu-domain.yaml
 *   resources/rules/fdu-ip.yaml
 */
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const BASE = "https://raw.githubusercontent.com/AkiraSalvare/fdu-netrules/refs/heads/main/clash_list";
const RULES_DIR = join(ROOT, "resources", "rules");

const checkOnly = process.argv.includes("--check");

/** 本地额外补充的域名（上游未覆盖但属于复旦体系） */
const EXTRA_DOMAINS = [
  "DOMAIN-SUFFIX,fdu.edu.cn",
  "DOMAIN-SUFFIX,fudanpress.com",
];

async function fetchLines(file) {
  const response = await fetch(`${BASE}/${file}`);
  if (!response.ok) {
    throw new Error(`拉取 ${file} 失败：HTTP ${response.status}`);
  }
  return (await response.text())
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith("#"));
}

function renderDomain(domains) {
  const lines = [
    "# 复旦及附属组织域名（rule-provider，classical/yaml）",
    "# 来源：https://github.com/AkiraSalvare/fdu-netrules + 官方域名核对",
    "# 更新：node scripts/sync-rules.mjs；修改后需同步 docs/RULES.md",
    "payload:",
    "  # 复旦主域（含所有子域：教务、邮箱、图书馆、ehall、elearning、uis、my 等）",
  ];
  for (const domain of domains) {
    lines.push(`  - ${domain}`);
  }
  lines.push("");
  return lines.join("\n");
}

function renderIp(ips) {
  const lines = [
    "# 复旦内网 IP（rule-provider，classical/yaml）",
    "# 需求核心：10.0.0.0/8 全部走校内 VPN（校内时则由 🎓 校内 组切到 DIRECT）",
    "payload:",
  ];
  for (const ip of ips) {
    lines.push(`  - ${ip}`);
  }
  lines.push("");
  return lines.join("\n");
}

function mergeUnique(list) {
  return Array.from(new Set(list));
}

async function main() {
  console.log("[sync-rules] 拉取上游规则…");
  const [domainLines, ipLines] = await Promise.all([
    fetchLines("FDUDomain.list"),
    fetchLines("FDUIP.list"),
  ]);
  const domains = mergeUnique([...domainLines, ...EXTRA_DOMAINS]).sort();
  const ips = mergeUnique(ipLines).sort();

  const targets = [
    { file: "fdu-domain.yaml", content: renderDomain(domains) },
    { file: "fdu-ip.yaml", content: renderIp(ips) },
  ];

  let changed = false;
  for (const target of targets) {
    const path = join(RULES_DIR, target.file);
    let previous = "";
    try {
      previous = readFileSync(path, "utf8");
    } catch {
      previous = "";
    }
    if (previous !== target.content) {
      changed = true;
      if (checkOnly) {
        console.log(`✗ ${target.file} 需要更新`);
      } else {
        writeFileSync(path, target.content);
        console.log(`✓ 已更新 ${target.file}`);
      }
    } else {
      console.log(`✓ ${target.file} 已是最新`);
    }
  }

  if (checkOnly && changed) {
    process.exit(1);
  }
  console.log("[sync-rules] 完成");
}

main().catch((error) => {
  console.error(`[sync-rules] ❌ ${error.message}`);
  process.exit(1);
});
