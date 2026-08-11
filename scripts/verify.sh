#!/usr/bin/env bash
# 一键验证 nekosearch 单机部署能搜到内容（重点：各编程语言官网）。
# 流程：构建 -> 起服务(演示数据) -> 查询若干语言 -> 断言返回对应官网 -> 校验自动补全。
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
  if curl -fsS "http://127.0.0.1:$PORT/search?q=rust&top_k=1" >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done

pass=0
fail=0
check() {
  local label="$1"; local url="$2"; local expect="$3"
  local resp
  resp=$(curl -fsS "http://127.0.0.1:$PORT$url")
  if printf '%s' "$resp" | grep -q -- "$expect"; then
    echo "[verify] PASS  $label -> 命中 $expect"
    pass=$((pass+1))
  else
    echo "[verify] FAIL  $label -> 未命中 $expect"
    fail=$((fail+1))
  fi
}

echo "[verify] 查询各编程语言官网 ..."
check "search rust"        "/search?q=rust&top_k=5"        "rust-lang.org"
check "search go"          "/search?q=go&top_k=5"          "go.dev"
check "search python"      "/search?q=python&top_k=5"      "python.org"
check "search java"        "/search?q=java&top_k=5"        "java.com"
check "search c/c++"       "/search?q=cppreference&top_k=5" "cppreference.com"
check "search kotlin"      "/search?q=kotlin&top_k=5"      "kotlinlang.org"
check "search swift"       "/search?q=swift&top_k=5"       "swift.org"
check "search node"        "/search?q=node&top_k=5"        "nodejs.org"

echo "[verify] 校验查询词自动补全 /suggest ..."
SUG=$(curl -fsS "http://127.0.0.1:$PORT/suggest?q=ru&limit=8")
if printf '%s' "$SUG" | grep -q "rust"; then
  echo "[verify] PASS  suggest 'ru' -> 含 'rust'"
  pass=$((pass+1))
else
  echo "[verify] FAIL  suggest 'ru' -> 不含 'rust'"
  fail=$((fail+1))
fi

echo "------------------------------"
if [ "$fail" -eq 0 ]; then
  echo "[verify] ALL PASS ($pass checks)"
  exit 0
else
  echo "[verify] FAIL: $fail/$((pass+fail)) checks failed"
  echo "--- server log ---"
  cat /tmp/nekosearch-verify.log
  exit 1
fi
