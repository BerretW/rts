@echo off
setlocal

set CARGO=%USERPROFILE%\.cargo\bin\cargo.exe

echo [BUILD] Server (debug)...
"%CARGO%" build -p rts-server
if errorlevel 1 (
    echo.
    echo [CHYBA] Build serveru selhal.
    pause
    exit /b 1
)

echo.
echo [OK] Build hotov. Spustitelny soubor: target\debug\rts-server.exe
