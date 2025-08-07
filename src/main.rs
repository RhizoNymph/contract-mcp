mod config;
mod ethereum;
mod server;

use anyhow::Result;
use clap::{Arg, Command};
use config::Config;
use server::ContractMcpServer;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging to stderr (important for MCP stdio servers)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let matches = Command::new("contract-mcp")
        .version("0.1.0")
        .about("MCP server for Ethereum smart contract interactions")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Path to configuration file"),
        )
        .arg(
            Arg::new("network")
                .short('n')
                .long("network")
                .value_name("NETWORK")
                .help("Default network to use (ethereum, sepolia, polygon, arbitrum)"),
        )
        .arg(
            Arg::new("rpc-url")
                .short('r')
                .long("rpc-url")
                .value_name("URL")
                .help("RPC endpoint URL"),
        )
        .arg(
            Arg::new("allow-writes")
                .long("allow-writes")
                .help("Allow write operations (transactions)")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("generate-config")
                .long("generate-config")
                .help("Generate a sample configuration file and exit")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("config-path")
                .long("config-path")
                .help("Print the default configuration file path and exit")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    // Handle special commands first
    if matches.get_flag("generate-config") {
        let sample_config = Config::generate_sample();
        println!("{}", sample_config);
        return Ok(());
    }

    if matches.get_flag("config-path") {
        match Config::default_config_path() {
            Ok(path) => {
                println!("{}", path.display());
                return Ok(());
            }
            Err(e) => {
                error!("Could not determine default config path: {}", e);
                return Err(e);
            }
        }
    }

    // Load configuration
    let config_path = matches.get_one::<String>("config").map(|s| s.as_str());
    let mut config = Config::load_or_default(config_path).await;

    // Override with command line arguments
    if let Some(network) = matches.get_one::<String>("network") {
        config.default_network = network.clone();
    }

    if let Some(rpc_url) = matches.get_one::<String>("rpc-url") {
        if let Some(network_config) = config.networks.get_mut(&config.default_network) {
            network_config.rpc_url = rpc_url.clone();
        }
    }

    if matches.get_flag("allow-writes") {
        config.security.allow_write_operations = true;
    }

    info!("Starting Contract MCP Server");
    info!("Default network: {}", config.default_network);
    info!(
        "Write operations allowed: {}",
        config.security.allow_write_operations
    );

    // Create and run the server
    let server = ContractMcpServer::new(config)?;

    if let Err(e) = server.run().await {
        error!("Server error: {}", e);
        return Err(e);
    }

    Ok(())
}
