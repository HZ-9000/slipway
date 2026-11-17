use std::{env, fs, io, path::PathBuf};

pub mod nix;

pub fn config_dir() -> io::Result<PathBuf> {
    let home = env::var_os("HOME")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
    Ok(PathBuf::from(home).join(".config/slipway"))
}

pub fn configure_app() -> anyhow::Result<()> {
    let config_directory = config_dir()?;
    fs::create_dir_all(config_directory.as_path())?;

    println!("Nix (linked library): {}", nix::eval::linked_version());

    match nix::install::nix_cli_version() {
        Ok(version) => println!("Nix (on PATH):        {version}"),
        Err(err) => println!("Nix (on PATH):        unavailable ({err:#})"),
    }

    match nix::eval::Nix::open().and_then(|nix| nix.store_version()) {
        Ok(version) => println!("Nix (store):          {version}"),
        Err(err) => println!("Nix (store):          unavailable ({err:#})"),
    }

    Ok(())
}
