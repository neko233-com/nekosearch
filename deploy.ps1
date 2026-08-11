# nekosearch 傻瓜式部署脚本 (Windows / PowerShell)
# 自动选择 docker 或 cargo 启动单机全角色（--role all）。
# 用法：在仓库根目录执行  .\deploy.ps1
Write-Host "==> nekosearch 傻瓜式部署"

$docker = Get-Command docker -ErrorAction SilentlyContinue
if ($docker) {
    Write-Host "[deploy] 检测到 docker，使用容器部署（单机全角色）..."
    if (docker compose version 2>$null) {
        docker compose up -d --build
    } else {
        docker-compose up -d --build
    }
    Write-Host ""
    Write-Host "完成！搜索引擎已启动："
    Write-Host "  检索接口 : http://localhost:7800/search?q=你的关键词"
    Write-Host "  注册中心 : http://localhost:7700/nodes"
    Write-Host "  停止     : docker compose down"
}
elseif (Get-Command cargo -ErrorAction SilentlyContinue) {
    Write-Host "[deploy] 未检测到 docker，使用 cargo 直接运行（需已安装 Rust）..."
    Write-Host "  检索接口 : http://localhost:7800/search?q=你的关键词"
    cargo run --release
}
else {
    Write-Host "[deploy] 错误：未检测到 docker 也未检测到 cargo(Rust)。请先安装其中之一。" -ForegroundColor Red
    exit 1
}
