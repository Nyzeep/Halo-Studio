# test-all.ps1 —— 全量测试：sidecar cargo build/test --workspace + app pytest
# 用法：powershell -ExecutionPolicy Bypass -File "D:\Halo Studio ultra\scripts\test-all.ps1"
# 任一步骤失败即以非 0 退出码结束。兼容 Windows PowerShell 5.1。

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$sidecarDir = Join-Path $root "sidecar"
$appDir = Join-Path $root "app"
$python = Join-Path $root ".venv\Scripts\python.exe"
$pytestBaseTemp = Join-Path $root (".scratch\pytest-test-all-" + [Guid]::NewGuid().ToString("N"))

$failures = @()

# ---- 1. Rust：构建 + 测试（workspace 全量） -----------------------------------
Push-Location $sidecarDir
try {
    Write-Host "[test-all] cargo build --workspace"
    cargo build --workspace
    if ($LASTEXITCODE -ne 0) {
        $failures += "cargo build --workspace（退出码 $LASTEXITCODE）"
    }
    else {
        Write-Host "[test-all] cargo test --workspace"
        cargo test --workspace
        if ($LASTEXITCODE -ne 0) {
            $failures += "cargo test --workspace（退出码 $LASTEXITCODE）"
        }
    }
}
finally {
    Pop-Location
}

# ---- 2. Python：app 全量 pytest ------------------------------------------------
if (-not (Test-Path $python)) {
    $failures += "未找到虚拟环境 Python：$python"
}
else {
    Push-Location $appDir
    try {
        Write-Host "[test-all] pytest tests -q --basetemp <workspace>"
        $env:PYTHONIOENCODING = "utf-8"
        & $python -m pytest tests -q --basetemp $pytestBaseTemp
        if ($LASTEXITCODE -ne 0) {
            $failures += "app pytest（退出码 $LASTEXITCODE）"
        }
    }
    finally {
        Pop-Location
    }
}

# ---- 3. 汇总 -------------------------------------------------------------------
if ($failures.Count -gt 0) {
    Write-Host ""
    Write-Host "[test-all] 失败项：" -ForegroundColor Red
    foreach ($f in $failures) {
        Write-Host "  - $f" -ForegroundColor Red
    }
    exit 1
}

Write-Host ""
Write-Host "[test-all] 全部通过：cargo build/test --workspace + app pytest" -ForegroundColor Green
exit 0
