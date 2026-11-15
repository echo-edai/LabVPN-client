// 只读比较 TUN、核心 SOCKS 和校园 SOCKS，握手成功必须继续验证 SSH 数据。
import net from "node:net";
import { execFileSync } from "node:child_process";
import { mkdirSync, readdirSync, writeFileSync } from "node:fs";
import os from "node:os";

const targets = process.argv.slice(2);
if (!targets.length || targets.some((host) => !net.isIPv4(host))) {
  console.error("用法：node scripts/audit-network.mjs <校内 IPv4> [其他 IPv4…]");
  process.exit(1);
}
const command = (file, args) => {
  try {
    return execFileSync(file, args, { encoding: "utf8", timeout: 8000 }).trim();
  } catch (error) {
    return { error: error.code ?? error.message, stderr: String(error.stderr ?? "").trim() };
  }
};

async function probe(host, proxyPort) {
  const started = Date.now();
  const result = {
    host,
    path: proxyPort ? `socks:${proxyPort}` : "system-route",
    connected: false,
    socksAccepted: false,
  };
  return new Promise((resolve) => {
    const socket = net.createConnection({ host: proxyPort ? "127.0.0.1" : host, port: proxyPort ?? 22 });
    let buffer = Buffer.alloc(0);
    let phase = proxyPort ? "greeting" : "banner";
    let done = false;
    const finish = (error) => {
      if (done) return;
      done = true;
      clearTimeout(deadline);
      socket.destroy();
      resolve({ ...result, elapsedMs: Date.now() - started, error });
    };
    const deadline = setTimeout(() => finish(`timeout:${phase}`), 9000);
    socket.on("connect", () => {
      result.connected = true;
      if (proxyPort) socket.write(Buffer.from([5, 1, 0]));
    });
    socket.on("data", (data) => {
      buffer = Buffer.concat([buffer, data]);
      if (buffer.length > 8192) return finish("response-too-large");
      if (phase === "greeting" && buffer.length >= 2) {
        if (buffer[0] !== 5 || buffer[1] !== 0) return finish("socks-auth-rejected");
        buffer = buffer.subarray(2);
        phase = "connect";
        socket.write(Buffer.from([5, 1, 0, 1, ...host.split(".").map(Number), 0, 22]));
      }
      if (phase === "connect" && buffer.length >= 5) {
        if (buffer[0] !== 5 || buffer[1] !== 0) return finish(`socks-reply:${buffer[1]}`);
        const size = buffer[3] === 1 ? 10 : buffer[3] === 4 ? 22 : buffer[3] === 3 ? 7 + buffer[4] : 0;
        if (!size) return finish("invalid-socks-address");
        if (buffer.length < size) return;
        result.socksAccepted = true;
        buffer = buffer.subarray(size);
        phase = "banner";
      }
      if (phase === "banner" && buffer.includes(10)) {
        const banner = buffer
          .toString("utf8")
          .split(/\r?\n/)
          .slice(0, -1)
          .find((line) => /^SSH-(2\.0|1\.99)-/.test(line));
        if (banner) {
          result.banner = banner.slice(0, 160);
          finish(null);
        } else if (buffer.length > 1024) finish("invalid-ssh-banner");
      }
    });
    socket.on("error", (error) => finish(error.code ?? error.message));
    socket.on("end", () => finish(`eof:${phase}`));
  });
}

const snapshot = {
  observedAt: new Date().toISOString(),
  proxy: command("/usr/sbin/scutil", ["--proxy"]),
  vpn: command("/usr/sbin/scutil", ["--nc", "list"]),
  processes: command("/bin/ps", ["-axo", "pid,ppid,comm"]),
  routes: command("/usr/sbin/netstat", ["-rn", "-f", "inet"]),
  extensions: command("/usr/bin/systemextensionsctl", ["list"]),
  networkServices: [],
  installedCandidates: [],
  startupCandidates: [],
  probes: [],
};
const candidate =
  /labvpn|clash|shadowrocket|easyconnect|motionpro|sing-box|v2ray|surge|quantumult|leigod|tailscale|wireguard|openvpn|anyconnect|forti|vpn/i;
for (const directory of ["/Applications", `${os.homedir()}/Applications`]) {
  try {
    snapshot.installedCandidates.push(...readdirSync(directory).filter((name) => candidate.test(name)));
  } catch {}
}
for (const directory of ["/Library/LaunchDaemons", "/Library/LaunchAgents", `${os.homedir()}/Library/LaunchAgents`]) {
  try {
    snapshot.startupCandidates.push(...readdirSync(directory).filter((name) => candidate.test(name)));
  } catch {}
}
const services = command("/usr/sbin/networksetup", ["-listallnetworkservices"]);
if (typeof services === "string") {
  for (const service of services
    .split("\n")
    .slice(1)
    .filter((name) => name && !name.startsWith("*"))) {
    const proxy = {};
    for (const setting of [
      "-getwebproxy",
      "-getsecurewebproxy",
      "-getsocksfirewallproxy",
      "-getautoproxyurl",
      "-getproxyautodiscovery",
      "-getproxybypassdomains",
    ]) {
      proxy[setting.slice(4)] = command("/usr/sbin/networksetup", [setting, service]);
    }
    snapshot.networkServices.push({ service, proxy });
  }
}
if (typeof snapshot.processes === "string") {
  snapshot.processes = snapshot.processes
    .split("\n")
    .filter((line) => /labvpn|mihomo|clash|shadowrocket|easyconnect|motionpro|sing-box|leigod|\/ssh$/i.test(line));
}
for (const host of targets) {
  snapshot.probes.push(...(await Promise.all([undefined, 7897, 11080].map((port) => probe(host, port)))));
}
mkdirSync("work/audit", { recursive: true });
const output = `work/audit/network-${Date.now()}.json`;
writeFileSync(output, JSON.stringify(snapshot, null, 2), { mode: 0o600 });
console.log(JSON.stringify({ output, probes: snapshot.probes }, null, 2));
