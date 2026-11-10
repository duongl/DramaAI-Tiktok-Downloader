#Requires -Version 5.1
$ErrorActionPreference = "Stop"

$guiDir = $PSScriptRoot
$repoRoot = Split-Path -Parent $guiDir
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
$cargo = Join-Path $cargoBin "cargo.exe"

if (-not (Test-Path $cargo)) {
    throw @"
Rust chưa được cài. Cài rustup với toolchain GNU trước:

  https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-gnu/rustup-init.exe
  rustup-init.exe -y --profile minimal --default-host x86_64-pc-windows-gnu

Sau đó chạy lại script này.
"@
}

$env:Path = "$cargoBin;$env:Path"

$hostLine = (& rustc -vV | Select-String '^host:').ToString()
if ($hostLine -notmatch 'windows-gnu') {
    Write-Host "[!] Toolchain hiện tại không phải windows-gnu. Chỉ cần Visual Studio C++ Build Tools." -ForegroundColor Yellow
    Push-Location $guiDir
    try {
        & cargo build --release
        if ($LASTEXITCODE -ne 0) { throw "cargo build thất bại." }
    } finally {
        Pop-Location
    }
    Write-Host "[OK] Đã build: $guiDir\target\release\DramaAI-Tiktok-Downloader.exe" -ForegroundColor Green
    exit 0
}

$sysroot = (& rustc --print sysroot).Trim()
$selfContained = Join-Path $sysroot "lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained"
$toolchainRoot = Join-Path $env:LOCALAPPDATA "drama-toolchain"
$msysBin = Join-Path $toolchainRoot "msys2-binutils\mingw64\bin"
$shimDir = Join-Path $toolchainRoot "dlltool-shim"

if (-not (Test-Path (Join-Path $msysBin "dlltool.exe"))) {
    Write-Host "[*] Tải binutils tối thiểu từ MSYS2 (một lần)..." -ForegroundColor Cyan
    $base = "https://repo.msys2.org/mingw/mingw64/"
    $packages = @(
        "mingw-w64-x86_64-binutils-2.47-4-any.pkg.tar.zst",
        "mingw-w64-x86_64-gettext-runtime-1.0-1-any.pkg.tar.zst",
        "mingw-w64-x86_64-zlib-1.4-2-any.pkg.tar.zst",
        "mingw-w64-x86_64-zstd-1.5.7-2-any.pkg.tar.zst",
        "mingw-w64-x86_64-libiconv-1.19.1-1-any.pkg.tar.zst"
    )
    $extractDir = Join-Path $toolchainRoot "msys2-binutils"
    New-Item -ItemType Directory -Force -Path $extractDir | Out-Null
    foreach ($package in $packages) {
        $file = Join-Path $env:TEMP $package
        if (-not (Test-Path $file)) {
            Invoke-WebRequest -Uri ($base + $package) -OutFile $file -UseBasicParsing
        }
        & tar -xf $file -C $extractDir
        if ($LASTEXITCODE -ne 0) { throw "Giải nén thất bại: $package" }
    }
}

if (-not (Test-Path (Join-Path $shimDir "dlltool.exe"))) {
    Write-Host "[*] Tạo dlltool shim..." -ForegroundColor Cyan
    New-Item -ItemType Directory -Force -Path $shimDir | Out-Null
    Copy-Item (Join-Path $msysBin "as.exe") $shimDir -Force
    Copy-Item (Join-Path $msysBin "dlltool.exe") (Join-Path $shimDir "dlltool-real.exe") -Force
    Copy-Item (Join-Path $msysBin "*.dll") $shimDir -Force
    & rustc -O --edition 2021 (Join-Path $guiDir "tools\dlltool_shim.rs") -o (Join-Path $shimDir "dlltool.exe")
    if ($LASTEXITCODE -ne 0) { throw "Không build được dlltool shim." }
}

$env:Path = "$msysBin;$shimDir;$selfContained;$env:Path"
$env:RUSTFLAGS = "-C dlltool=$(Join-Path $shimDir 'dlltool.exe')"

Push-Location $guiDir
try {
    & cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build thất bại." }
} finally {
    Pop-Location
}

Write-Host ""
Write-Host "[OK] Đã build: $guiDir\target\release\DramaAI-Tiktok-Downloader.exe" -ForegroundColor Green
Write-Host "Chạy: run_gui.bat"
