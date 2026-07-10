@echo off
setlocal

cd /d "%~dp0src\frontend" || exit /b 1
if not exist node_modules call npm install --no-audit --no-fund || exit /b 1

cd /d "%~dp0src\backend" || exit /b 1
call "%~dp0src\frontend\node_modules\.bin\tauri.cmd" dev
endlocal
