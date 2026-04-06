@echo off
setlocal

set CARGO=%USERPROFILE%\.cargo\bin\cargo.exe

echo [RUN] Kompilace a spusteni clienta (debug)...
"%CARGO%" run -p game
if errorlevel 1 (
    echo.
    echo [CHYBA] Spusteni clienta selhalo.
    pause
    exit /b 1
)
