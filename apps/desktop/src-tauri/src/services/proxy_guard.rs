//! 只读识别常见代理进程；连接流程不再终止其他软件。

use crate::platform;

/// 仅读取可执行文件名，不收集可能含凭据的进程参数。
pub fn running_proxies() -> Result<Vec<String>, String> {
    #[cfg(unix)]
    let output = platform::run_command("ps", &["-axo", "comm="]);
    #[cfg(windows)]
    let output = platform::run_command("tasklist", &["/FO", "CSV", "/NH"]);
    let output = output.ok_or_else(|| "无法读取进程列表".to_string())?;
    let mut found = Vec::new();
    for line in output.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("labvpn") {
            continue;
        }
        for (name, patterns) in [
            ("Clash / Mihomo", &["clash", "mihomo"][..]),
            ("Shadowrocket", &["shadowrocket"][..]),
            (
                "EasyConnect",
                &["easyconnect", "ecagent", "easymonitor"][..],
            ),
            ("MotionPro", &["motionpro", "/vpnd"][..]),
            ("LeiGod", &["leigod"][..]),
            ("sing-box / Hiddify", &["sing-box", "hiddify"][..]),
            ("V2Ray / Xray", &["v2ray", "xray"][..]),
            ("Surge / Stash", &["surge", "stash.app"][..]),
            ("fdu-connect (external)", &["fdu-connect"][..]),
        ] {
            if patterns.iter().any(|pattern| lower.contains(pattern)) {
                found.push(name.to_string());
            }
        }
    }
    found.sort();
    found.dedup();
    Ok(found)
}
