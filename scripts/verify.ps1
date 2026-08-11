# 一键验证 nekosearch 搜索是否可用：构建 -> 起服务(演示数据) -> 查询 -> 断言非空。
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$bin = "target/release/nekosearch.exe"
if (-not (Test-Path $bin)) {
  Write-Host "[verify] building nekosearch ..."
  cargo build --release
}

$port = 7800
Write-Host "[verify] starting nekosearch --role all --seed-demo on :$port ..."
$log = Join-Path $env:TEMP "neko-verify.log"
$proc = Start-Process -FilePath $bin -ArgumentList "--role", "all", "--seed-demo" `
  -PassThru -RedirectStandardOutput $log -RedirectStandardError $log
try {
  $ready = $false
  for ($i = 0; $i -lt 40; $i++) {
    try {
      Invoke-RestMethod -Uri "http://127.0.0.1:$port/search?q=nekosearch&top_k=1" -TimeoutSec 2 | Out-Null
      $ready = $true
      break
    } catch {
      Start-Sleep -Milliseconds 500
    }
  }
  if (-not $ready) { throw "server did not become ready; see $log" }

  Write-Host "[verify] querying GET /search?q=nekosearch&top_k=5 ..."
  $resp = Invoke-RestMethod -Uri "http://127.0.0.1:$port/search?q=nekosearch&top_k=5" -TimeoutSec 5
  $resp | ConvertTo-Json -Depth 4 | Write-Host

  $count = @($resp).Count
  if ($count -gt 0) {
    Write-Host "[verify] PASS: $count result(s) returned"
    exit 0
  } else {
    Write-Host "[verify] FAIL: empty results"
    exit 1
  }
} finally {
  Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
}
