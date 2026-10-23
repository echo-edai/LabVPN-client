# fdu-connect（校内 VPN sidecar）——本地补丁说明

上游：[AkiraSalvare/fdu-connect](https://github.com/AkiraSalvare/fdu-connect)（AGPL-3.0，zju-connect 的复旦分支）
基线 commit：`a904877f0eda0c7b1d7933ad6e80d5e8d3ffd584`
补丁文件：`robust-socks.patch`（本文档同目录）

本目录是 **上游基线 + 补丁** 的完整源码（AGPL-3.0 §6 "Corresponding Source"）。
`scripts/fetch-sidecars.mjs` 会优先用它编译出随 App 打包的 `fdu-connect` 二进制。

## 为什么需要补丁

RVPN 隧道在用户态 gVisor 栈里完成 TCP 三次握手。实验与日志（`helper/core.log`）显示：

- 目标内网主机（例如 `10.134.142.226:22`）在某些时段握手会被丢弃/延迟，gVisor 按退避重传 SYN
  （默认 RTO 下限 1s，几秒内只有 2~3 次机会），且 `gonet.DialTCP` **没有超时、也不可取消**
  （黑洞目标实测 >120s 不返回）；
- LabVPN 的 mihomo 侧对代理拨号有 5 秒超时（`C.DefaultTCPTimeout`），于是每次慢握手都被记为
  `dial 🎓 校内 ... error: context deadline exceeded`，并在 fdu-connect 侧留下 CLOSE_WAIT
  半开连接与泄漏 goroutine。

即：**能连上、只是慢**的连接被 5 秒超时误杀，且失败会持续累积资源。

实测还发现一个关键现象：卡住的旧握手即使路径已经恢复也不会自己恢复，而**新连接往往 ms 级成功**
（新的源端口 → RVPN 服务端新的 NAT 表项）。因此「一次长超时」不够，需要**分段重试**。

## 补丁内容（`robust-socks.patch`）

1. **可取消的握手**：`stack.Stack` 新增 `DialTCPContext(ctx, addr)`；
   gVisor 实现改用 `gonet.DialContextTCP`，上下文取消即中止握手（不泄漏 goroutine/连接）。
   TUN 栈实现同步补齐（Darwin/Linux/Windows）。
2. **更积极的 SYN 重传**（`stack/gvisor/stack.go`）：把 gVisor 的 `TCPMinRTOOption`
   从默认 1s 下调到 500ms（仍远大于校内 RTT 的十几毫秒），提高「路径短暂抖动」时的握手成功率。
3. **SOCKS5 宽限应答 + 分段重试**（`service/graceful.go`）：
   - 握手在 `socks_early_reply_ms`（默认 2000ms）内完成 → 行为与上游完全一致（严格 SOCKS5）；
   - 超过该时间仍未完成 → 先给客户端回 `succeeded`，后续 `Read/Write` 阻塞等待真实连接；
   - 单次握手限制 4s（`connectAttemptTimeout`）。若超时/失败，则**新建一个 gVisor 连接**
     （新的源端口 + 新的 RVPN 服务端 NAT 表项）重试，直到 `socks_connect_timeout`
     （默认 20s）总预算用完；
   - 客户端断开时 `Close()` 会 `cancel` 当前握手，不留孤儿连接。
   - 两个坑已处理：
     a. gVisor 出错时返回「typed-nil `*gonet.TCPConn` + error」，直接塞进 `net.Conn` 会在
        `Close()` 时 nil 解引用崩溃 → 记录结果前归一化为 nil；
     b. 握手完成前 `LocalAddr()` 必须返回**合法 IPv4**（`0.0.0.0`）；零值 `&net.TCPAddr{}`
        （IP 为 nil）会让 SOCKS5 应答少 4 字节，客户端会一直等到自己的超时（实测 mihomo 5s）。
4. **两个新配置项**（TOML / CLI 同名）：
   - `socks_connect_timeout`（`-socks-connect-timeout`，秒，代码默认 15，LabVPN 生成配置固定 20；0=不限）
   - `socks_early_reply_ms`（`-socks-early-reply-ms`，毫秒，默认 2000，0=严格 SOCKS5）
   LabVPN 生成的 `config.toml` 会显式写入这两个值。
5. 单元测试：`service/graceful_test.go`（快/慢/失败/提前关闭/超时/typed-nil/LocalAddr/重试/预算 九条路径）。

未改动任何登录、隧道协议、加密与资源解析逻辑。

## 如何更新上游

```bash
git clone https://github.com/AkiraSalvare/fdu-connect.git /tmp/fdu-connect
cd /tmp/fdu-connect && git checkout <new-commit>
git apply --3way /path/to/vendor/fdu-connect/robust-socks.patch   # 若冲突按上文语义调整
# 复核 go test -vet=off ./service/ 后，把源码覆盖回本目录并重新生成补丁
```

> 注意：上游 `go.mod` 要求 Go ≥ 1.24；用 Go 1.25+ 编译会因 `sagernet/sing` 的
> `net.errNoSuchInterface` linkname 变化而链接失败（上游依赖问题，与本补丁无关）。
