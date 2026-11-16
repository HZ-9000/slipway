use clap::{Parser, Subcommand};
use log::info;
use slipway::get_configuration;

#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug, Clone)]
enum Commands {
    Test {
        #[arg(short, long)]
        list: bool,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    info!("Welcome To Slipway!");

    let cli = Cli::parse();

    // You can check for the existence of subcommands, and if found use their
    // matches just as you would the top level cmd
    match cli.command {
        Commands::Test { list } => {
            if list {
                println!("Printing testing lists...");
            } else {
                get_configuration();
            }
        }
    }

    Ok(())
}
