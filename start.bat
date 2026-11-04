@echo off
setlocal

cd /d "%~dp0"

:: Cargo / Rust Pfad sicherstellen
where cargo >nul 2>&1
if errorlevel 1 (
    if exist "C:\Users\it-experte\.cargo\bin\cargo.exe" (
        set "PATH=C:\Users\it-experte\.cargo\bin;%PATH%"
    )
)

set "TARGET_EXE=C:\Users\it-experte\.cargo\target_csv_analyzer\release\configurable_csv_analyzer.exe"

:: Cargo prueft Fingerprints & Slint-Aenderungen vollautomatisch.
:: Wenn nichts geaendert wurde, ueberspringt Cargo den Build in ca. 0.1 Sekunden.
cargo build --release
if errorlevel 1 (
    echo.
    echo [FEHLER] Kompilierung fehlgeschlagen.
    pause
    exit /b 1
)

:: Fertiges Programm starten
start "" "%TARGET_EXE%" %*
