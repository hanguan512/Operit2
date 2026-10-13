# Operit2 Linux 执行节点（shell 版）

无桌面依赖的 Linux 执行节点：在本机运行 Operit2 Core，监听 WebSocket，与手机 App 配对后作为 Agent 的执行节点（终端命令、文件操作、HTTP 等全部工具能力）。

## 编译

在目标 Linux 机器上（需要 Rust 1.75+）：

```sh
sudo apt install -y build-essential pkg-config   # 或对应发行版等价包
cd linux
./build.sh
```

产物：`linux/target/release/operit-linux-node`

## 运行

```sh
./target/release/operit-linux-node            # 前台运行，默认监听 0.0.0.0:37195
```

可选参数：

| 参数 | 说明 | 默认 |
|---|---|---|
| `--runtime <目录>` | 运行时数据目录 | `~/.operit2-node/runtime` |
| `--workspace <目录>` | Agent 工作区目录 | `~/.operit2-node/workspace` |
| `--bind <地址:端口>` | 监听地址 | `0.0.0.0:37195` |
| `--name <名称>` | 节点显示名称 | 主机名 |

启动后会打印节点 ID、监听地址和局域网连接地址，并保持前台运行（Ctrl-C 优雅停止）。

## 手机配对

1. 确保 Linux 机器与手机在同一局域网（或互通网络），放行 37195/tcp。
2. 手机 App → 菜单 → 执行节点 → 添加：
   - 协议选 **WebSocket**，填 Linux 机器的 `局域网IP:37195`
3. 点击节点进入详情 → **发起配对**。
4. Linux 终端会打印 **六位配对码**，输入到手机完成配对，之后自动建立加密会话并可在手机上设为当前执行节点。

配对凭据持久化在 `--runtime` 目录，重启节点无需重新配对。

## systemd 常驻（可选）

```ini
# /etc/systemd/system/operit-node.service
[Unit]
Description=Operit2 Linux execution node
After=network-online.target

[Service]
User=你的用户名
ExecStart=/opt/operit-linux-node
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

```sh
sudo systemctl enable --now operit-node
```

## 工具能力

节点内置标准工具集：终端命令（PTY shell）、文件系统读写、HTTP 请求、系统操作、Web 访问等；手机端聊天中由 Agent 通过函数调用使用。安全子集（toast/通知/应用/系统设置）依赖宿主桥，Linux 上按需实现。
