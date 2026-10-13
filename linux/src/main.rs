#![allow(non_snake_case)]

//! Headless Linux execution node for Operit2. Runs the embedded core, serves
//! the peer link over WebSocket, prints pairing codes to the terminal, and
//! pairs with the phone exactly like any other node.

use std::path::PathBuf;
use std::sync::Arc;

use operit_core_application::{CoreApplication, CoreApplicationConfig};
use operit_host_api::TimeUtils;
use operit_host_linux_native::LinuxTerminalHost;
use operit_host_linux_native::LinuxWebVisitHost;
use operit_link::protocol::LinkDeviceInfo;
use operit_node_runtime::NodeServices::PeerTransport;
use operit_node_runtime::PeerStateStore::{PeerHostConfig, PeerHostPortMode};

struct NodeState {
    app: CoreApplication,
    runtime: tokio::runtime::Runtime,
}

struct Args {
    runtimeRoot: PathBuf,
    workspaceRoot: PathBuf,
    bindAddress: String,
    name: String,
}

fn parseArgs() -> Args {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let mut args = Args {
        runtimeRoot: PathBuf::from(&home).join(".operit2-node").join("runtime"),
        workspaceRoot: PathBuf::from(&home).join(".operit2-node").join("workspace"),
        bindAddress: "0.0.0.0:37195".to_string(),
        name: hostnameFallback(),
    };
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--runtime" => {
                if let Some(value) = iter.next() {
                    args.runtimeRoot = PathBuf::from(value);
                }
            }
            "--workspace" => {
                if let Some(value) = iter.next() {
                    args.workspaceRoot = PathBuf::from(value);
                }
            }
            "--bind" => {
                if let Some(value) = iter.next() {
                    args.bindAddress = value;
                }
            }
            "--name" => {
                if let Some(value) = iter.next() {
                    args.name = value;
                }
            }
            "--help" | "-h" => {
                printHelp();
                std::process::exit(0);
            }
            other => {
                eprintln!("未知参数 {other}（--help 查看用法）");
                std::process::exit(1);
            }
        }
    }
    args
}

fn hostnameFallback() -> String {
    std::fs::read_to_string("/etc/hostname")
        .map(|name| name.trim().to_string())
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "linux-node".to_string())
}

fn printHelp() {
    println!(
        "Operit2 Linux 执行节点\n\n用法: operit-linux-node [选项]\n\n选项:\n  --runtime <目录>    运行时数据目录（默认 ~/.operit2-node/runtime）\n  --workspace <目录>  工作区目录（默认 ~/.operit2-node/workspace）\n  --bind <地址:端口>  监听地址（默认 0.0.0.0:37195）\n  --name <名称>       节点显示名称（默认主机名）\n  -h, --help          显示帮助\n\n启动后用手机 App 的 执行节点 → 添加 输入本机局域网 IP:端口 完成配对。"
    );
}

/// Best-effort LAN address discovery without sending any packet.
fn lanAddress() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip().to_string())
}

fn main() {
    let args = parseArgs();
    let runtimeRoot = args.runtimeRoot.clone();
    let workspaceRoot = args.workspaceRoot.clone();
    let bindAddress = args.bindAddress.clone();
    let name = args.name.clone();

    println!("Operit2 Linux 执行节点");
    println!("  运行时目录: {}", runtimeRoot.display());
    println!("  工作区目录: {}", workspaceRoot.display());
    println!("  监听地址:   {}", bindAddress);

    if let Err(error) = std::fs::create_dir_all(&runtimeRoot) {
        eprintln!("创建运行时目录失败: {error}");
        std::process::exit(1);
    }
    if let Err(error) = std::fs::create_dir_all(&workspaceRoot) {
        eprintln!("创建工作区目录失败: {error}");
        std::process::exit(1);
    }

    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("创建异步运行时失败: {error}");
            std::process::exit(1);
        }
    };

    let startResult = runtime.block_on(async move {
        let hostManager = operit_host_linux_native::createRuntimeHostManager(
            runtimeRoot,
            workspaceRoot,
            Arc::new(LinuxWebVisitHost::new()),
        )
        .withTerminalHost(Arc::new(LinuxTerminalHost::new()));

        let deviceInfo = LinkDeviceInfo {
            platform: "linux".to_string(),
            model: name,
            pairingIncarnation: None,
        };
        let app = CoreApplication::start(
            CoreApplicationConfig::new(hostManager, deviceInfo).withSpaceSync(false),
        )
        .await
        .map_err(|e| e.to_string())?;

        let services = app.accessServices();
        let mut config = services
            .localHostConfig()
            .map_err(|e| e.to_string())?
            .unwrap_or(PeerHostConfig {
                bindAddress: String::new(),
                token: uuid::Uuid::new_v4().to_string(),
                transports: Vec::new(),
                discoveryEnabled: true,
                portMode: PeerHostPortMode::Fixed,
                updatedAt: TimeUtils::currentTimeMillis(),
            });
        config.bindAddress = bindAddress;
        config.portMode = PeerHostPortMode::Fixed;
        config.transports = vec![PeerTransport::Tcp];
        config.updatedAt = TimeUtils::currentTimeMillis();
        services.saveLocalHostConfig(config).map_err(|e| e.to_string())?;
        services
            .startListening(vec![PeerTransport::Tcp])
            .await
            .map_err(|e| e.to_string())?;
        // headless 无 CLI/TUI，空间控制面必须在此初始化，否则无法审批加入请求
        let control = services.deviceSpaceControl()?;
        if !control.initialized {
            services.bootstrapDeviceSpaceControl()?;
        }
        Ok::<CoreApplication, String>(app)
    });

    let app = match startResult {
        Ok(app) => app,
        Err(error) => {
            eprintln!("核心启动失败: {error}");
            std::process::exit(1);
        }
    };

    let nodeId = app.localNodeId();
    let actualBind = app
        .accessServices()
        .localHostConfig()
        .ok()
        .flatten()
        .map(|config| config.bindAddress)
        .unwrap_or_default();
    let port = actualBind
        .rsplit(':')
        .next()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(37195);
    let lanIp = lanAddress().unwrap_or_else(|| "<本机IP>".to_string());

    println!();
    println!("节点已启动");
    println!("  节点 ID:  {nodeId}");
    println!("  监听:     {actualBind}");
    println!("  手机连接: 在 App 执行节点 → 添加 中填写 {lanIp}:{port}");
    println!("  配对码会打印在本终端，输入到手机即完成配对");
    println!("按 Ctrl-C 停止节点");
    println!();

    let runtimeHandle = runtime.handle().clone();
    runtime.block_on(async move {
        let mut seenPairings: Vec<String> = Vec::new();
        let mut seenJoins: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(1200));
        let shutdown = tokio::signal::ctrl_c();
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                _ = shutdown.as_mut() => {
                    println!();
                    println!("正在停止节点…");
                    break;
                }
                _ = tick.tick() => {
                    let prompts = {
                        let services = app.accessServices();
                        services.pairingPrompts().unwrap_or_else(|_| Vec::new())
                    };
                    for prompt in prompts {
                        if seenPairings.contains(&prompt.pairingId) {
                            continue;
                        }
                        seenPairings.push(prompt.pairingId.clone());
                        println!("收到配对请求: {} ({})", prompt.displayName, prompt.peerNodeId);
                        println!("配对码: {}  ← 输入到手机完成配对", prompt.confirmationCode);
                    }
                    // headless 自动审批：已配对设备申请并入本节点空间时直接批准
                    let joins = {
                        let services = app.accessServices();
                        services.incomingDeviceSpaceJoins().await.unwrap_or_default()
                    };
                    for join in joins {
                        if !join.canApprove || seenJoins.contains(&join.requestId) {
                            continue;
                        }
                        seenJoins.insert(join.requestId.clone());
                        let services = app.accessServices();
                        match services
                            .decideDeviceSpaceJoin(join.requestId.clone(), join.assignmentVersion, true)
                            .await
                        {
                            Ok(_) => println!(
                                "已自动批准空间加入: {} ({})",
                                join.applicantName, join.requestId
                            ),
                            Err(error) => {
                                println!("自动批准失败 {}: {error}", join.requestId);
                                seenJoins.remove(&join.requestId);
                            }
                        }
                    }
                }
            }
        }
        let _ = runtimeHandle;
        let _ = app.accessServices().stopListening().await;
        app.shutdown().await;
        println!("节点已停止");
    });
}
