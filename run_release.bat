@echo off
setlocal

set CARGO=%USERPROFILE%\.cargo\bin\cargo.exe

echo [RUN] Kompilace a spusteni (release)...
"%CARGO%" run --release
if errorlevel 1 (
    echo.
    echo [CHYBA] Spusteni selhalo.
    pause
    exit /b 1
)
