@echo off
setlocal

echo === MyNote tests ===

cd /d "%~dp0src\frontend" || exit /b 1
if not exist node_modules call npm install --no-audit --no-fund || exit /b 1
echo [1/3] svelte-check
call npm run check || exit /b 1

cd /d "%~dp0src\backend" || exit /b 1
echo [2/3] cargo test
cargo test || exit /b 1

if not exist "%~dp0output\mynote.exe" (
  echo [3/3] skipped: output\mynote.exe missing - run build.bat first
  exit /b 1
)
cd /d "%~dp0src\e2e" || exit /b 1
echo [3/3] e2e against output\mynote.exe
call npm test || exit /b 1

echo.
echo All tests passed.
endlocal
