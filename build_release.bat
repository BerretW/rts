@echo off
setlocal

set CARGO=%USERPROFILE%\.cargo\bin\cargo.exe

echo [BUILD] Release (muze trvat dele)...
"%CARGO%" build --release
if errorlevel 1 (
    echo.
    echo [CHYBA] Build selhal.
    pause
    exit /b 1
)

echo.
echo [OK] Release build hotov: target\release\rts.exe
