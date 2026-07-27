# dev.ps1 —— 开发启动：构建 Sidecar → 设置 HALO_SIDECAR_EXE → venv 启动应用
# 用法：powershell -ExecutionPolicy Bypass -File "D:\Halo Studio ultra\scripts\dev.ps1"
# 兼容 Windows PowerShell 5.1（不使用 && / ?? / 三元运算符）。

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$sidecarDir = Join-Path $root "sidecar"
$appDir = Join-Path $root "app"
$python = Join-Path $root ".venv\Scripts\python.exe"
$sidecarExe = Join-Path $sidecarDir "target\debug\halo-sidecar.exe"

if (-not (Test-Path $python)) {
    Write-Host "[dev] 未找到虚拟环境 Python：$python" -ForegroundColor Red
    Write-Host "[dev] 请先在项目根创建 .venv 并安装 app 依赖（pip install -e `"$appDir`"）"
    exit 1
}

Write-Host "[dev] 构建 Sidecar：cargo build -p halo-sidecar"
Push-Location $sidecarDir
try {
    cargo build -p halo-sidecar
    if ($LASTEXITCODE -ne 0) {
        Write-Host "[dev] Sidecar 构建失败（退出码 $LASTEXITCODE）" -ForegroundColor Red
        exit $LASTEXITCODE
    }
}
finally {
    Pop-Location
}

if (-not (Test-Path $sidecarExe)) {
    Write-Host "[dev] 构建成功但未找到可执行文件：$sidecarExe" -ForegroundColor Red
    exit 1
}

$env:HALO_SIDECAR_EXE = $sidecarExe
$env:PYTHONIOENCODING = "utf-8"
Write-Host "[dev] HALO_SIDECAR_EXE = $sidecarExe"
Write-Host "[dev] 启动应用：python -m halo_studio.main"

Push-Location $appDir
try {
    & $python -m halo_studio.main
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
