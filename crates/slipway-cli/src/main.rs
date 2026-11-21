use std::{
    io::{self, BufRead as _, Write as _},
    path::PathBuf,
    process::ExitCode,
};

use clap::{Parser, Subcommand};
use log::info;
use slipway_core::{
    NixVersions,
    package::PackageName,
    system::{ChannelSource, ConfigKind, System},
};

/// Set up and manage the software installed on a NixOS computer.
#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug, Clone)]
enum Commands {
    /// Show which NixOS this computer runs, where its system setup lives, and which
    /// versions of Nix (the package manager behind NixOS) slipway can find
    Detect {
        /// Where your system setup is, if it isn't in /etc/nixos
        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,
    },
    /// Connect your system setup to slipway, so the software you add with slipway
    /// gets installed. Shows what it adds and asks before changing anything
    Connect {
        /// Where your system setup is, if it isn't in /etc/nixos
        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,
        /// Make the change without asking first
        #[arg(short, long)]
        yes: bool,
    },
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
        Commands::Detect { config } => {
            print_system(&slipway_core::detect_system(config)?);
            print_nix_versions(&slipway_core::detect());
        }
        Commands::Connect { config, yes } => connect(config, yes)?,
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

fn connect(config: Option<PathBuf>, yes: bool) -> anyhow::Result<()> {
    let system = slipway_core::detect_system(config)?;
    let wiring = slipway_core::plan_connection(&system)?;

    match &wiring {
        None => println!(
            "{} already uses slipway, so it doesn't need changing.",
            system.config.entry.display()
        ),
        Some(wiring) => {
            println!("Slipway will add this to {}:\n", wiring.path.display());
            print!("{}", wiring.diff());
            println!();
            if !yes && !confirm("Make this change?")? {
                println!("Nothing was changed.");
                return Ok(());
            }
        }
    }

    slipway_core::connect(&system, wiring.as_ref())?;
    if wiring.is_some() {
        println!(
            "Connected. Slipway's settings live in {}.",
            slipway_core::config::MANAGED_DIR
        );
    }

    // Nix only sees the files git tracks in a configuration kept in git.
    if system.config.dir.join(".git").exists() {
        println!(
            "Your system setup is kept in git, so run `git -C {} add slipway` before rebuilding.",
            system.config.dir.display()
        );
    }
    Ok(())
}

/// Asks a yes/no question, taking anything but yes (including no answer at
/// all) as no.
fn confirm(question: &str) -> anyhow::Result<bool> {
    print!("{question} [y/N] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

fn print_system(system: &System) {
    let nixos = &system.nixos;
    match &nixos.build {
        Some(build) => println!(
            "NixOS:                {} (build {build})",
            nixos.pretty_name
        ),
        None => println!("NixOS:                {}", nixos.pretty_name),
    }

    let stability = if system.channel.is_unstable() {
        "unstable"
    } else {
        "stable"
    };
    let source = match system.channel_source {
        ChannelSource::FlakeLock => "from flake.lock",
        ChannelSource::BuildVersion => "from the NixOS version",
    };
    println!(
        "Channel:              {} ({stability}, {source})",
        system.channel
    );

    let kind = match system.config.kind {
        ConfigKind::Flake => "flake",
        ConfigKind::ConfigurationNix => "configuration.nix, no flake",
    };
    println!(
        "System config:        {} ({kind})",
        system.config.entry.display()
    );

    let on_off = |enabled| if enabled { "enabled" } else { "not enabled" };
    match &system.features {
        Ok(features) => {
            println!("nix-command:          {}", on_off(features.nix_command));
            println!("flakes:               {}", on_off(features.flakes));
        }
        Err(err) => println!("Nix features:         unavailable ({err:#})"),
    }
}

fn print_nix_versions(versions: &NixVersions) {
    match &versions.on_path {
        Ok(version) => println!("Nix (on PATH):        {version}"),
        Err(err) => println!("Nix (on PATH):        unavailable ({err:#})"),
    }

    match &versions.store {
        Ok(version) => println!("Nix (store):          {version}"),
        Err(err) => println!("Nix (store):          unavailable ({err:#})"),
    }
}
