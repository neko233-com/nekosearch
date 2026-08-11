#!/usr/bin/env bash
# nekosearch 傻瓜式部署脚本
# 自动选择 docker 或 cargo 启动单机全角色（--role all）。
set -euo pipefail

echo "==> nekosearch 傻瓜式部署"

if command -v docker >/dev/null 2>&1; then
  echo "[deploy] 检测到 docker，使用容器部署（单机全角色）..."
  if command -v docker >/dev/null 2>&1 && docker compose version >/dev/null 2>&1; then
    docker compose up -d --build
  else
    docker-compose up -d --build
  fi
  echo ""
  echo "完成！搜索引擎已启动："
  echo "  检索接口 : http://localhost:7800/search?q=你的关键词"
  echo "  注册中心 : http://localhost:7700/nodes"
  echo "  停止     : docker compose down"
elif command -v cargo >/dev/null 2>&1; then
  echo "[deploy] 未检测到 docker，使用 cargo 直接运行（需已安装 Rust）..."
  echo "  检索接口 : http://localhost:7800/search?q=你的关键词"
  cargo run --release
else
  echo "[deploy] 错误：未检测到 docker 也未检测到 cargo(Rust)。请先安装其中之一。" >&2
  exit 1
fi
