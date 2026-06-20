@echo off
setlocal enabledelayedexpansion

cd /d "%~dp0"

if /i "%~1"=="installer" (
    echo [BGWM] Building installer...
    powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0installer\build-installer.ps1"
    exit /b %ERRORLEVEL%
)
set "SETTINGS_FLAG="
set "BUILD_FLAG="
set "EXE_PATH=target\debug\bgwm.exe"

if /i "%~1"=="release" (
    set "BUILD_FLAG=--release"
    set "EXE_PATH=target\release\bgwm.exe"
)

if /i "%~1"=="settings" (
    set "SETTINGS_FLAG=--settings"
)

echo [BGWM] Building (%EXE_PATH%)...
cargo build %BUILD_FLAG%
if errorlevel 1 (
    echo [BGWM] Build failed.
    exit /b 1
)

echo [BGWM] Starting...
"%EXE_PATH% " "%SETTINGS_FLAG%"

exit /b %ERRORLEVEL%
