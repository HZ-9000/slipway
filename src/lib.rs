use std::{env, fs, io, path::PathBuf};

pub fn find_matches(
    content: &str,
    pattern: &str,
    mut writer: impl std::io::Write,
) -> std::io::Result<()> {
    for line in content.lines() {
        if line.contains(pattern) {
            writeln!(writer, "{}", line)?;
        }
    }
    Ok(())
}

pub fn config_dir() -> io::Result<PathBuf> {
    let home = env::var_os("HOME")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
    Ok(PathBuf::from(home).join(".config/slipway"))
}

pub fn configure_app() -> Result<(), std::io::Error> {
    let config_directory = config_dir()?;
    fs::create_dir_all(config_directory.as_path())?;
    Ok(())
}
