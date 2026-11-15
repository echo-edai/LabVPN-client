//! 启动恢复可重试，只有全部清理成功才允许连接。

use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::error::{CmdResult, CommandError};
use crate::state::AppState;

use super::{helper, proxy_lease};

/// 调用方必须持有连接事务锁；不创建核心或校园隧道。
pub async fn recover(state: &AppState) -> CmdResult<()> {
    state.startup_ready.store(false, Ordering::Release);
    let mut errors = Vec::new();
    let port = helper::default_port();
    // 端口有监听却无法认证时保留故障，不能误报已清理。
    let listening = tokio::time::timeout(
        Duration::from_millis(500),
        tokio::net::TcpStream::connect(("127.0.0.1", port)),
    )
    .await
    .is_ok_and(|result| result.is_ok());
    if listening {
        let stopped = async {
            let token = helper::read_token(&state.paths.token_file)?;
            helper::stop_core(port, &token).await?;
            Ok::<(), CommandError>(())
        }
        .await;
        if let Err(err) = stopped {
            errors.push(format!(
                "旧核心清理失败 / Previous core cleanup failed: {err}"
            ));
        }
    }
    if let Err(err) = proxy_lease::restore(&state.paths.data_dir) {
        errors.push(format!(
            "系统代理恢复失败 / Proxy restoration failed: {err}"
        ));
    }
    let mut config = state.config_snapshot();
    if config.engine.connect_on_launch || config.engine.close_other_proxies {
        config.engine.connect_on_launch = false;
        config.engine.close_other_proxies = false;
        if let Err(err) = state.save_config(config) {
            errors.push(err.to_string());
        }
    }
    let error = (!errors.is_empty()).then(|| errors.join("; "));
    if let Ok(mut current) = state.startup_error.write() {
        *current = error.clone();
    }
    if let Some(error) = error {
        state.logs.error("startup", error.clone());
        return Err(CommandError::new(
            "lifecycle",
            "清理未完成，请点击“安全断开并恢复”重试 / Cleanup incomplete; retry safe disconnect",
        )
        .with_detail(error));
    }
    state.startup_ready.store(true, Ordering::Release);
    state.logs.info(
        "startup",
        "干净启动完成：等待手动连接，不启动代理核心和校园隧道",
    );
    Ok(())
}
