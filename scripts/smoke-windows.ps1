# smoke-windows.ps1 —— Windows 原生烟测 + 技术路线静态红线断言
# 用法：powershell -ExecutionPolicy Bypass -File "D:\Halo Studio ultra\scripts\smoke-windows.ps1"
# 流程：构建 halo-sidecar → 设 HALO_SIDECAR_EXE → python -m halo_studio.main --smoke
#（默认平台；失败时用 QT_QPA_PLATFORM=offscreen 重试一次并注明）→ 断言 SMOKE-OK；
# 随后静态断言根入口以及 app/sidecar 无 electron/react/vite/webview 依赖、QML 无 WebEngine/WebView。
# 兼容 Windows PowerShell 5.1。

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$sidecarDir = Join-Path $root "sidecar"
$appDir = Join-Path $root "app"
$python = Join-Path $root ".venv\Scripts\python.exe"
$sidecarExe = Join-Path $sidecarDir "target\debug\halo-sidecar.exe"

$failures = @()

# ---- 1. 构建 halo-sidecar ------------------------------------------------------
Write-Host "[smoke] 构建 Sidecar：cargo build -p halo-sidecar"
Push-Location $sidecarDir
try {
    cargo build -p halo-sidecar
    if ($LASTEXITCODE -ne 0) {
        $failures += "halo-sidecar 构建失败（退出码 $LASTEXITCODE）"
    }
}
finally {
    Pop-Location
}

if (Test-Path $sidecarExe) {
    $env:HALO_SIDECAR_EXE = $sidecarExe
    Write-Host "[smoke] HALO_SIDECAR_EXE = $sidecarExe"
}
else {
    # Sidecar 不可用不属于烟测失败：应用须如实显示不可用原因且仍 SMOKE-OK
    #（issue 01 诚实状态要求）。此处不设 HALO_SIDECAR_EXE，仅记录事实。
    Write-Host "[smoke] 注意：未找到 $sidecarExe，烟测将在 Sidecar 不可用（界面如实显示原因）路径下进行" -ForegroundColor Yellow
}

# ---- 2. 运行 --smoke（默认平台；失败重试一次 offscreen） -----------------------
function Invoke-Smoke {
    param([string]$Label)
    Write-Host "[smoke] 运行 python -m halo_studio.main --smoke（$Label）"
    Push-Location $appDir
    try {
        $out = & $python -m halo_studio.main --smoke
        $code = $LASTEXITCODE
    }
    finally {
        Pop-Location
    }
    $text = ($out | Out-String)
    return @{ Code = $code; Text = $text }
}

if (-not (Test-Path $python)) {
    $failures += "未找到虚拟环境 Python：$python，无法运行烟测"
    $smokeOk = $false
}
else {
    $env:PYTHONIOENCODING = "utf-8"
    $r = Invoke-Smoke -Label "默认平台"
    $smokeOk = ($r.Code -eq 0 -and $r.Text -match "SMOKE-OK")
    if (-not $smokeOk) {
        Write-Host "[smoke] 默认平台失败（退出码 $($r.Code)），使用 QT_QPA_PLATFORM=offscreen 重试一次" -ForegroundColor Yellow
        $env:QT_QPA_PLATFORM = "offscreen"
        try {
            $r = Invoke-Smoke -Label "offscreen 重试"
            $smokeOk = ($r.Code -eq 0 -and $r.Text -match "SMOKE-OK")
            if ($smokeOk) {
                Write-Host "[smoke] 注明：本次 SMOKE-OK 来自 QT_QPA_PLATFORM=offscreen 重试" -ForegroundColor Yellow
            }
        }
        finally {
            Remove-Item Env:QT_QPA_PLATFORM -ErrorAction SilentlyContinue
        }
    }
}

if ($smokeOk) {
    Write-Host "[smoke] [PASS] --smoke 输出 SMOKE-OK 且退出码 0" -ForegroundColor Green
}
else {
    $failures += "--smoke 未输出 SMOKE-OK 或退出码非 0"
    Write-Host "[smoke] [FAIL] --smoke 未输出 SMOKE-OK 或退出码非 0" -ForegroundColor Red
}

# ---- 3. 静态红线断言 -----------------------------------------------------------
# 断言输出格式统一：[PASS]/[FAIL] + 结论。任何 FAIL 计入 $failures。

$bannedDeps = "(?i)\b(electron|react|vite|webview)\b"

function Assert-FilesClean {
    param([string]$Label, [System.IO.FileInfo[]]$Files, [string]$Pattern)
    $hits = @()
    foreach ($f in $Files) {
        $m = Select-String -Path $f.FullName -Pattern $Pattern
        if ($m) { $hits += $m }
    }
    if ($hits.Count -eq 0) {
        Write-Host "[assert] [PASS] $Label" -ForegroundColor Green
        return $true
    }
    Write-Host "[assert] [FAIL] $Label" -ForegroundColor Red
    foreach ($h in $hits) {
        Write-Host "         命中：$($h.Path):$($h.LineNumber): $($h.Line.Trim())" -ForegroundColor Red
    }
    return $false
}

# 3.1 app/pyproject.toml 无 electron/react/vite/webview 依赖
$pyproject = Get-Item (Join-Path $appDir "pyproject.toml")
if (-not (Assert-FilesClean -Label "app\pyproject.toml 无 electron/react/vite/webview 依赖" -Files @($pyproject) -Pattern $bannedDeps)) {
    $failures += "app pyproject 含被禁依赖"
}

# 3.2 sidecar 全部 Cargo.toml 无 electron/react/vite/webview 依赖
$cargoTomls = @(Get-Item (Join-Path $sidecarDir "Cargo.toml"))
$cargoTomls += Get-ChildItem -Path (Join-Path $sidecarDir "crates") -Recurse -Filter "Cargo.toml" -File
if (-not (Assert-FilesClean -Label "sidecar 全部 Cargo.toml 无 electron/react/vite/webview 依赖" -Files $cargoTomls -Pattern $bannedDeps)) {
    $failures += "sidecar Cargo.toml 含被禁依赖"
}

# 3.3 QML import 扫描：无 WebEngine / WebView
$qmlFiles = @(Get-ChildItem -Path (Join-Path $appDir "halo_studio\qml") -Recurse -Filter "*.qml" -File)
if ($qmlFiles.Count -eq 0) {
    Write-Host "[assert] [FAIL] QML 目录为空，无法断言" -ForegroundColor Red
    $failures += "QML 目录为空"
}
elseif (-not (Assert-FilesClean -Label "QML 无 WebEngine/WebView（共 $($qmlFiles.Count) 个文件）" -Files $qmlFiles -Pattern "WebEngine|WebView")) {
    $failures += "QML 含 WebEngine/WebView"
}

# 3.4 根 package.json 仅允许保留只读参考元数据，不能继续提供旧前端运行入口。
$rootPackage = Join-Path $root "package.json"
$rootLegacyPattern = '(?i)"(main|scripts|dependencies|devDependencies)"\s*:|\b(electron|react|vite|webview)\b'
if (Test-Path -LiteralPath $rootPackage) {
    $rootPackageFile = Get-Item -LiteralPath $rootPackage
    if (-not (Assert-FilesClean -Label "根 package.json 无旧前端运行入口或依赖" -Files @($rootPackageFile) -Pattern $rootLegacyPattern)) {
        $failures += "根 package.json 保留旧前端运行入口或依赖"
    }
}
else {
    Write-Host "[assert] [PASS] 根目录不存在 package.json（无 npm 前端运行入口）" -ForegroundColor Green
}

# 3.5 app 与 sidecar 目录无 package.json（electron/react/vite 均经 npm 引入）
$pkgJson = @()
$pkgJson += Get-ChildItem -Path $appDir -Recurse -Filter "package.json" -File -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -notmatch "\\(target|__pycache__|\.venv)\\" }
$pkgJson += Get-ChildItem -Path $sidecarDir -Recurse -Filter "package.json" -File -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -notmatch "\\target\\" }
if ($pkgJson.Count -eq 0) {
    Write-Host "[assert] [PASS] app 与 sidecar 目录不存在 package.json（无 npm 前端依赖入口）" -ForegroundColor Green
}
else {
    Write-Host "[assert] [FAIL] 发现 package.json：$($pkgJson.FullName -join '; ')" -ForegroundColor Red
    $failures += "存在 package.json"
}

# ---- 4. 汇总 -------------------------------------------------------------------
Write-Host ""
if ($failures.Count -gt 0) {
    Write-Host "[smoke] 失败项：" -ForegroundColor Red
    foreach ($f in $failures) {
        Write-Host "  - $f" -ForegroundColor Red
    }
    exit 1
}
Write-Host "[smoke] 烟测与全部静态断言通过" -ForegroundColor Green
exit 0
