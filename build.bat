@echo off
setlocal

echo === MyNote build ===

cd /d "%~dp0src\frontend" || exit /b 1
echo [1/2] npm install
call npm install --no-audit --no-fund || exit /b 1

cd /d "%~dp0src\backend" || exit /b 1
echo [2/2] tauri build (runs frontend build, then cargo release)
call "%~dp0src\frontend\node_modules\.bin\tauri.cmd" build --no-bundle || exit /b 1

if not exist "%~dp0output" mkdir "%~dp0output" || exit /b 1
copy /y "%~dp0src\backend\target\release\MyNote.exe" "%~dp0output\MyNote.exe" >nul || exit /b 1

echo.
echo Done: %~dp0output\MyNote.exe
endlocal
