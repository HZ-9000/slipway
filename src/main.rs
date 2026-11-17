use clap::{Parser, Subcommand};
use log::info;
use slipway::configure_app;

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
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    info!("Welcome To Slipway!");

    let cli = Cli::parse();

    match cli.command {
        Commands::Configure { init } => {
            if init {
                configure_app()?;
            } else {
                println!("Unknown Command");
            }
        }
    }

    Ok(())
}
