@echo off
setlocal

set CARGO=%USERPROFILE%\.cargo\bin\cargo.exe

echo [BUILD] Debug...
"%CARGO%" build
if errorlevel 1 (
    echo.
    echo [CHYBA] Build selhal.
    pause
    exit /b 1
)

echo.
echo [OK] Build hotov. Spustitelny soubor: target\debug\rts.exe
