@echo off
setlocal
cd /d "%~dp0"

:: ==============================================================================
:: PROJEKT: Configurable CSV Analyzer
:: DATEI:   start-configurable-csv-analyzer-windows.bat
:: ZWECK:   Automatischer Build und Start der Desktop-Anwendung unter Windows
:: ==============================================================================

set "APP_NAME=Configurable CSV Analyzer"
set "BIN_NAME=configurable_csv_analyzer"
set "BUILD_PROFILE=release"
title %APP_NAME%

echo [%APP_NAME%] Initialisiere Umgebung...

:: 1. Cargo & Rust Toolchain auf R:\ und System sicherstellen
if not defined RUSTUP_LOG set "RUSTUP_LOG=error"
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

:: 2. ImDisk/RAM-Disk Junction fuer Cargo Registry absichern
if exist "R:\.cargo" (
    if not exist "%USERPROFILE%\.cargo\registry" mkdir "%USERPROFILE%\.cargo\registry" 2>nul
    fsutil reparsepoint query "R:\.cargo\registry" >nul 2>&1
    if errorlevel 1 (
        if exist "R:\.cargo\registry" rmdir /s /q "R:\.cargo\registry" 2>nul
        mklink /J "R:\.cargo\registry" "%USERPROFILE%\.cargo\registry" >nul 2>&1
    )
)

:: 3. Lokale Pfad-Abhaengigkeiten absichern (falls vorhanden)
if not exist "..\slint-desktop-kit\Cargo.toml" (
    if exist "%USERPROFILE%\Projekte\slint-desktop-kit\Cargo.toml" (
        mklink /J "..\slint-desktop-kit" "%USERPROFILE%\Projekte\slint-desktop-kit" >nul 2>&1
    ) else if exist "%USERPROFILE%\git\slint-desktop-kit\Cargo.toml" (
        mklink /J "..\slint-desktop-kit" "%USERPROFILE%\git\slint-desktop-kit" >nul 2>&1
    ) else (
        echo [%APP_NAME%] Klone fehlende Abhaengigkeit slint-desktop-kit...
        git clone https://github.com/1337userxyz/slint-desktop-kit.git "..\slint-desktop-kit" >nul 2>&1
    )
)

:: 4. Build-Bereinigung vor dem Kompilieren (schont RAM-Disk R:\)
if "%BUILD_PROFILE%"=="release" (
    if exist "target\debug" (
        echo [%APP_NAME%] Bereinige ungenutzte Debug-Builddaten [target\debug]...
        rmdir /s /q "target\debug" 2>nul
    )
    if exist "target\release\deps\%BIN_NAME%-*.pdb" (
        del /f /q "target\release\deps\%BIN_NAME%-*.pdb" 2>nul
    )
    set "CARGO_BUILD_FLAG=--release"
    set "TARGET_EXE=%~dp0target\release\%BIN_NAME%.exe"
) else (
    set "CARGO_BUILD_FLAG="
    set "TARGET_EXE=%~dp0target\debug\%BIN_NAME%.exe"
)

:: Manueller Schalter --clean: Vollstaendige Neu-Kompilierung erzwingen
if /i "%~1"=="--clean" (
    echo [%APP_NAME%] Schalter --clean erkannt: Bereinige target vollstaendig...
    cargo clean
    shift
)

:: 5. Kompilierung
echo [%APP_NAME%] Pruefe / Baue %BUILD_PROFILE%...
cargo build %CARGO_BUILD_FLAG% --bin %BIN_NAME%
if errorlevel 1 (
    echo.
    echo [%APP_NAME%] [FEHLER] Kompilierung fehlgeschlagen.
    echo Falls der MSVC-Linker fehlt, starte bitte:
    echo   R:\git\pwsh7-scripts\Setup\DevWorkstation\Apps\VisualStudioBuildTools\Install-VisualStudioBuildTools.bat
    pause
    exit /b 1
)

:: 6. Fertiges Programm starten
echo [%APP_NAME%] Starte %BIN_NAME%...
start "" "%TARGET_EXE%" %*
