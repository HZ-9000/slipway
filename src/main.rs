use std::process::ExitCode;

use clap::{Parser, Subcommand};
use log::info;
use slipway_core::{NixVersions, package::PackageName};

/// Set up and manage the software installed on a NixOS computer.
#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug, Clone)]
enum Commands {
    /// Show which versions of Nix (the package manager behind NixOS) slipway can find
    Detect,
    /// Add a package (a piece of software) to your system setup
    Add {
        /// Name of the package, for example `firefox` or `python3Packages.requests`
        package: PackageName,
    },
    /// Remove a package from your system setup
    Remove {
        /// Name of the package, for example `firefox`
        package: PackageName,
    },
    /// List the packages in your system setup
    List,
    /// Check your system setup for problems without changing anything
    Check,
    /// Set up slipway's own settings folder
    Configure {
        /// Create the settings folder (~/.config/slipway) and show what slipway can find
        #[arg(short, long, required = true)]
        init: bool,
    },
}

fn main() -> ExitCode {
    env_logger::init();
    info!("Welcome To Slipway!");

    let cli = Cli::parse();

    match run(cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Commands) -> anyhow::Result<()> {
    match command {
        Commands::Detect => print_nix_versions(&slipway_core::detect()),
        Commands::Add { package } => slipway_core::add(&package)?,
        Commands::Remove { package } => slipway_core::remove(&package)?,
        Commands::List => {
            for package in slipway_core::list()? {
                println!("{package}");
            }
        }
        Commands::Check => slipway_core::check()?,
        Commands::Configure { init: _ } => {
            let config_directory = slipway_core::init_config_dir()?;
            println!("Settings folder:      {}", config_directory.display());
            print_nix_versions(&slipway_core::detect());
        }
    }

    Ok(())
}

fn print_nix_versions(versions: &NixVersions) {
    println!("Nix (linked library): {}", versions.linked);

    match &versions.on_path {
        Ok(version) => println!("Nix (on PATH):        {version}"),
        Err(err) => println!("Nix (on PATH):        unavailable ({err:#})"),
    }

    match &versions.store {
        Ok(version) => println!("Nix (store):          {version}"),
        Err(err) => println!("Nix (store):          unavailable ({err:#})"),
    }
}
