#!/usr/bin/env bash
# 一键验证 nekosearch 搜索是否可用：构建 -> 起服务(演示数据) -> 查询 -> 断言非空。
set -euo pipefail

cd "$(dirname "$0")/.."

BIN="target/release/nekosearch"
if [ ! -x "$BIN" ]; then
  echo "[verify] building nekosearch ..."
  cargo build --release
fi

PORT=7800
echo "[verify] starting nekosearch --role all --seed-demo on :$PORT ..."
"$BIN" --role all --seed-demo > /tmp/nekosearch-verify.log 2>&1 &
PID=$!
trap 'kill $PID 2>/dev/null || true' EXIT

# 等待服务就绪
for _ in $(seq 1 40); do
  if curl -fsS "http://127.0.0.1:$PORT/search?q=nekosearch&top_k=1" >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done

echo "[verify] querying GET /search?q=nekosearch&top_k=5 ..."
RESP=$(curl -fsS "http://127.0.0.1:$PORT/search?q=nekosearch&top_k=5")
echo "$RESP"

COUNT=$(printf '%s' "$RESP" | grep -o '"doc"' | wc -l | tr -d ' ')
if [ "${COUNT:-0}" -gt 0 ]; then
  echo "[verify] PASS: $COUNT result(s) returned"
  exit 0
else
  echo "[verify] FAIL: empty results"
  echo "--- server log ---"
  cat /tmp/nekosearch-verify.log
  exit 1
fi
