use clap::{Parser, Subcommand};
use log::info;
use slipway::{config_dir, configure_app};

#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug, Clone)]
enum Commands {
    Configure {
        #[arg(short, long)]
        init: bool,
        #[arg(short, long)]
        get: bool,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    info!("Welcome To Slipway!");

    let cli = Cli::parse();

    // You can check for the existence of subcommands, and if found use their
    // matches just as you would the top level cmd
    match cli.command {
        Commands::Configure { init, get } => {
            if init {
                configure_app()?;
            } else if get {
                config_dir()?;
            } else {
                println!("Unknown Command");
            }
        }
    }

    Ok(())
}
