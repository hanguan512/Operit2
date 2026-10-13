#!/usr/bin/env bash
# 在 Linux 目标机上编译执行节点（需已安装 Rust 1.75+ 与系统依赖 libudev 可选）。
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release
echo "产物: $(pwd)/target/release/operit-linux-node"
