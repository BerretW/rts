@echo off
setlocal

set CARGO=%USERPROFILE%\.cargo\bin\cargo.exe

echo [BUILD] Client (debug)...
"%CARGO%" build -p game
if errorlevel 1 (
    echo.
    echo [CHYBA] Build clienta selhal.
    pause
    exit /b 1
)

echo.
echo [OK] Build hotov. Spustitelny soubor: target\debug\rts.exe
