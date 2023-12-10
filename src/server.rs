mod node;

use std::fs;
use clap::{Parser, Args, Subcommand};
use node::config::Config;
use crate::node::daft_server::DaftServer;


#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct DaftCli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {

    /// Start server
    Server(ServerArgs),
}

#[derive(Args)]
struct ServerArgs {

    /// Path to server config
    #[arg(short, long)]
    config: String,
}

async fn start_server(server_args: &ServerArgs) {
    let raw_config = fs::read_to_string(&server_args.config)
        .expect("Valid path to config");
    let config: Config = serde_json::from_str(&raw_config)
        .expect("Valid config value");
    let daft_server = DaftServer::new(config);

    daft_server.start().await;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let daft_cli = DaftCli::parse();

    match &daft_cli.command {
        Commands::Server(server_args) => start_server(server_args)
    }.await;

    Ok(())
}
