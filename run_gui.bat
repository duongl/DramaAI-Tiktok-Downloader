@echo off
setlocal
cd /d "%~dp0"

set "EXE=gui\target\release\DramaAI-Tiktok-Downloader.exe"
if not exist "%EXE%" (
    echo Chua co ban build.
    echo Chay: powershell -ExecutionPolicy Bypass -File gui\build.ps1
    pause
    exit /b 1
)

where python >nul 2>nul
if errorlevel 1 (
    echo Khong tim thay Python tren PATH.
    echo Cai Python 3.8+ hoac dat bien DRAMA_PYTHON tro toi python.exe.
    pause
    exit /b 1
)

start "" "%EXE%"
