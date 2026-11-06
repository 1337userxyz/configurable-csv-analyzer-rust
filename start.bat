@echo off
setlocal

cd /d "%~dp0"

:: Unterdruecke rustup-Warnungen zu nicht-kanonischen Pfaden auf virtuellen/ImDisk-Laufwerken (z.B. R:\)
if not defined RUSTUP_LOG set "RUSTUP_LOG=error"

:: Cargo / Rust Pfad sicherstellen (unterstuetzt R:\.cargo\bin sowie Userprofile)
where cargo >nul 2>&1
if errorlevel 1 (
    if defined CARGO_HOME (
        if exist "%CARGO_HOME%\bin\cargo.exe" set "PATH=%CARGO_HOME%\bin;%PATH%"
    )
    if exist "R:\.cargo\bin\cargo.exe" (
        set "PATH=R:\.cargo\bin;%PATH%"
    ) else if exist "%USERPROFILE%\.cargo\bin\cargo.exe" (
        set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
    )
)

set "TARGET_EXE=%~dp0target\release\configurable_csv_analyzer.exe"

:: Cargo prueft Fingerprints & Slint-Aenderungen vollautomatisch.
:: Wenn nichts geaendert wurde, ueberspringt Cargo den Build in ca. 0.1 Sekunden.
cargo build --release
if errorlevel 1 (
    echo.
    echo [FEHLER] Kompilierung fehlgeschlagen.
    echo Falls der Fehler "linker link.exe not found" lautet, starte bitte:
    echo   R:\git\pwsh7-scripts\Setup\DevWorkstation\Apps\VisualStudioBuildTools\Install-VisualStudioBuildTools.bat
    pause
    exit /b 1
)

:: Fertiges Programm starten
start "" "%TARGET_EXE%" %*
