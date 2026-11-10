# Creates a copyable shortcut in the project folder and on the Desktop.
$ErrorActionPreference = "Stop"

$root = $PSScriptRoot
$exe = Join-Path $root "gui\target\release\DramaAI-Tiktok-Downloader.exe"
$icon = Join-Path $root "gui\assets\icon.ico"

if (-not (Test-Path $exe)) {
    throw "Chua build GUI. Chay: powershell -ExecutionPolicy Bypass -File gui\build.ps1"
}
if (-not (Test-Path $icon)) {
    throw "Khong tim thay icon: $icon"
}

$desktop = [Environment]::GetFolderPath("Desktop")
$shortcutPaths = @(
    (Join-Path $root "DramaAI Tiktok Downloader.lnk"),
    (Join-Path $desktop "DramaAI Tiktok Downloader.lnk")
)

$shell = New-Object -ComObject WScript.Shell
foreach ($lnkPath in $shortcutPaths) {
    $shortcut = $shell.CreateShortcut($lnkPath)
    $shortcut.TargetPath = $exe
    $shortcut.WorkingDirectory = $root
    $shortcut.IconLocation = $icon
    $shortcut.Description = "DramaAI Tiktok Downloader"
    $shortcut.WindowStyle = 1
    $shortcut.Save()
    Write-Host "Created shortcut: $lnkPath"
}
