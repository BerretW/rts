@echo off
setlocal

set CARGO=%USERPROFILE%\.cargo\bin\cargo.exe

echo [RUN] Kompilace a spusteni serveru (debug)...
"%CARGO%" run -p rts-server
if errorlevel 1 (
    echo.
    echo [CHYBA] Spusteni serveru selhalo.
    pause
    exit /b 1
)
