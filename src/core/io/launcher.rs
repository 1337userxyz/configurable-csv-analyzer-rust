use std::path::{Path, PathBuf};

pub fn find_libreoffice_binary() -> Option<PathBuf> {
    let candidates = [
        r"C:\Program Files\LibreOffice\program\soffice.exe",
        r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
        "/usr/bin/libreoffice",
        "/usr/bin/soffice",
    ];
    kit_launcher::find_binary("soffice", &candidates)
        .or_else(|| kit_launcher::find_in_path("libreoffice"))
}

pub fn open_in_libreoffice(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let soffice = find_libreoffice_binary()
        .ok_or("LibreOffice (soffice.exe) wurde nicht in Standardpfaden oder im PATH gefunden.")?;

    kit_launcher::open_in_external_app(&soffice, path, None)?;
    Ok(())
}

pub fn find_notepad_plus_plus_binary() -> Option<PathBuf> {
    let candidates = [
        r"C:\Program Files\Notepad++\notepad++.exe",
        r"C:\Program Files (x86)\Notepad++\notepad++.exe",
        r"C:\ProgramData\chocolatey\bin\notepad++.exe",
    ];
    kit_launcher::find_binary("notepad++", &candidates)
}

pub fn open_in_external_app(
    app: &crate::core::model::ExternalAppConfig,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let raw_cmd = app.command_or_path.trim();

    let resolved_binary = if !raw_cmd.is_empty() && Path::new(raw_cmd).exists() {
        PathBuf::from(raw_cmd)
    } else if app.name.eq_ignore_ascii_case("LibreOffice")
        || raw_cmd.to_ascii_lowercase().contains("soffice")
    {
        find_libreoffice_binary().ok_or_else(|| {
            format!(
                "LibreOffice wurde unter '{}' oder im System nicht gefunden.",
                raw_cmd
            )
        })?
    } else if app.name.eq_ignore_ascii_case("Notepad++")
        || raw_cmd.to_ascii_lowercase().contains("notepad++")
    {
        find_notepad_plus_plus_binary().ok_or_else(|| {
            format!(
                "Notepad++ wurde unter '{}' oder im System nicht gefunden.",
                raw_cmd
            )
        })?
    } else if !raw_cmd.is_empty() {
        kit_launcher::find_in_path(raw_cmd).unwrap_or_else(|| PathBuf::from(raw_cmd))
    } else {
        return Err(format!("Kein Programmpfad für '{}' konfiguriert.", app.name).into());
    };

    let args_str = app.arguments.trim();
    kit_launcher::open_in_external_app(
        &resolved_binary,
        path,
        if args_str.is_empty() {
            None
        } else {
            Some(args_str)
        },
    )?;
    Ok(())
}
