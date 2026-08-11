# 一键验证 nekosearch 单机部署能搜到内容（重点：各编程语言官网）。
# 流程：构建 -> 起服务(演示数据) -> 查询若干语言 -> 断言返回对应官网 -> 校验自动补全。
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$bin = "target/release/nekosearch.exe"
if (-not (Test-Path $bin)) {
  Write-Host "[verify] building nekosearch ..."
  cargo build --release
}

$port = 7512
Write-Host "[verify] starting nekosearch --role all --seed-demo on :$port ..."
$log = Join-Path $env:TEMP "neko-verify.log"
$proc = Start-Process -FilePath $bin -ArgumentList "--role", "all", "--seed-demo" `
  -PassThru -RedirectStandardOutput $log -RedirectStandardError $log
try {
  $ready = $false
  for ($i = 0; $i -lt 40; $i++) {
    try {
      Invoke-RestMethod -Uri "http://127.0.0.1:$port/search?q=rust&top_k=1" -TimeoutSec 2 | Out-Null
      $ready = $true; break
    } catch { Start-Sleep -Milliseconds 500 }
  }
  if (-not $ready) { throw "server did not become ready; see $log" }

  $pass = 0; $fail = 0
  $enc = { param($s) [System.Uri]::EscapeDataString($s) }

  function Check($label, $q, $expect) {
    $url = "http://127.0.0.1:$port/search?q=$(&$enc $q)&top_k=5"
    try {
      $resp = Invoke-RestMethod -Uri $url -TimeoutSec 5
      $urls = @($resp) | ForEach-Object { $_.doc.url }
      if ($urls -match [regex]::Escape($expect)) {
        Write-Host "[verify] PASS  $label -> 命中 $expect"; $script:pass++
      } else {
        Write-Host "[verify] FAIL  $label -> 未命中 $expect"; $script:fail++
      }
    } catch {
      Write-Host "[verify] FAIL  $label -> 请求错误: $_"; $script:fail++
    }
  }

  Write-Host "[verify] 查询各编程语言官网 ..."
  Check "search rust"   "rust"        "rust-lang.org"
  Check "search go"     "go"          "go.dev"
  Check "search python" "python"      "python.org"
  Check "search java"   "java"        "java.com"
  Check "search c/c++"  "cppreference" "cppreference.com"
  Check "search kotlin" "kotlin"      "kotlinlang.org"
  Check "search swift"  "swift"       "swift.org"
  Check "search node"   "node"        "nodejs.org"

  Write-Host "[verify] 校验查询词自动补全 /suggest ..."
  try {
    $sug = Invoke-RestMethod -Uri "http://127.0.0.1:$port/suggest?q=ru&limit=8" -TimeoutSec 5
    if (@($sug) -contains "rust") {
      Write-Host "[verify] PASS  suggest 'ru' -> 含 'rust'"; $script:pass++
    } else {
      Write-Host "[verify] FAIL  suggest 'ru' -> 不含 'rust (got: $($sug -join ', '))'"; $script:fail++
    }
  } catch {
    Write-Host "[verify] FAIL  suggest 请求错误: $_"; $script:fail++
  }

  Write-Host "------------------------------"
  if ($fail -eq 0) {
    Write-Host "[verify] ALL PASS ($pass checks)"
    exit 0
  } else {
    Write-Host "[verify] FAIL: $fail/$($pass+$fail) checks failed"
    exit 1
  }
} finally {
  Stop-Process -Id $proc.Id -Force -ErrorActionSilentlyContinue
}
