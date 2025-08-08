use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub networks: HashMap<String, NetworkConfig>,
    pub default_network: String,
    pub security: SecurityConfig,
    pub server: ServerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub rpc_url: String,
    pub chain_id: u64,
    pub explorer_url: Option<String>,
    pub gas: GasConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasConfig {
    pub default_gas_limit: u64,
    pub max_gas_price: Option<u64>,
    pub priority_fee: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub allow_write_operations: bool,
    pub require_confirmation: bool,
    pub max_transaction_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub transport: String,
    pub stdio: StdioConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdioConfig {
    pub buffer_size: Option<usize>,
}

impl Default for Config {
    fn default() -> Self {
        let mut networks = HashMap::new();

        networks.insert(
            "ethereum".to_string(),
            NetworkConfig {
                rpc_url: "https://eth-mainnet.g.alchemy.com/v2/demo".to_string(),
                chain_id: 1,
                explorer_url: Some("https://etherscan.io".to_string()),
                gas: GasConfig {
                    default_gas_limit: 100000,
                    max_gas_price: Some(50_000_000_000), // 50 Gwei
                    priority_fee: Some(2_000_000_000),   // 2 Gwei
                },
            },
        );

        networks.insert(
            "sepolia".to_string(),
            NetworkConfig {
                rpc_url: "https://eth-sepolia.g.alchemy.com/v2/demo".to_string(),
                chain_id: 11155111,
                explorer_url: Some("https://sepolia.etherscan.io".to_string()),
                gas: GasConfig {
                    default_gas_limit: 100000,
                    max_gas_price: Some(20_000_000_000), // 20 Gwei
                    priority_fee: Some(1_000_000_000),   // 1 Gwei
                },
            },
        );

        networks.insert(
            "polygon".to_string(),
            NetworkConfig {
                rpc_url: "https://polygon-mainnet.g.alchemy.com/v2/demo".to_string(),
                chain_id: 137,
                explorer_url: Some("https://polygonscan.com".to_string()),
                gas: GasConfig {
                    default_gas_limit: 100000,
                    max_gas_price: Some(500_000_000_000), // 500 Gwei
                    priority_fee: Some(30_000_000_000),   // 30 Gwei
                },
            },
        );

        networks.insert(
            "arbitrum".to_string(),
            NetworkConfig {
                rpc_url: "https://arb-mainnet.g.alchemy.com/v2/demo".to_string(),
                chain_id: 42161,
                explorer_url: Some("https://arbiscan.io".to_string()),
                gas: GasConfig {
                    default_gas_limit: 100000,
                    max_gas_price: Some(5_000_000_000), // 5 Gwei
                    priority_fee: Some(100_000_000),    // 0.1 Gwei
                },
            },
        );

        Self {
            networks,
            default_network: "ethereum".to_string(),
            security: SecurityConfig {
                allow_write_operations: false,
                require_confirmation: true,
                max_transaction_value: None,
            },
            server: ServerConfig {
                transport: "stdio".to_string(),
                stdio: StdioConfig {
                    buffer_size: Some(1024 * 1024), // 1MB buffer
                },
            },
        }
    }
}

impl Config {
    /// Load configuration from a TOML file
    pub async fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| anyhow!("Failed to read config file {:?}: {}", path, e))?;

        let config: Config = toml::from_str(&content)
            .map_err(|e| anyhow!("Failed to parse config file {:?}: {}", path, e))?;

        Ok(config)
    }

    /// Save configuration to a TOML file
    pub async fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        let content = toml::to_string_pretty(self)
            .map_err(|e| anyhow!("Failed to serialize config: {}", e))?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).await.map_err(|e| {
                    anyhow!("Failed to create config directory {:?}: {}", parent, e)
                })?;
            }
        }

        fs::write(path, content)
            .await
            .map_err(|e| anyhow!("Failed to write config file {:?}: {}", path, e))?;

        Ok(())
    }

    /// Load configuration with fallback to default
    pub async fn load_or_default<P: AsRef<Path>>(path: Option<P>) -> Self {
        let mut config = match path {
            Some(path) => match Self::load_from_file(path).await {
                Ok(config) => {
                    tracing::info!("Loaded configuration from file");
                    config
                }
                Err(e) => {
                    tracing::warn!("Failed to load config file, using defaults: {}", e);
                    Self::default()
                }
            },
            None => Self::default(),
        };

        // Apply environment variable substitutions
        config.apply_env_vars();
        config
    }

    /// Add a new network configuration
    pub fn add_network(&mut self, name: String, config: NetworkConfig) {
        self.networks.insert(name, config);
    }

    /// Apply environment variable substitutions to configuration
    fn apply_env_vars(&mut self) {
        // Check for ALCHEMY_API_KEY environment variable
        if let Ok(api_key) = std::env::var("ALCHEMY_API_KEY") {
            tracing::info!("Using ALCHEMY_API_KEY environment variable for RPC URLs");

            for (network_name, network_config) in &mut self.networks {
                // Replace Alchemy demo URLs with actual API key
                if network_config.rpc_url.contains("alchemy.com/v2/demo") {
                    network_config.rpc_url = network_config
                        .rpc_url
                        .replace("/demo", &format!("/{}", api_key));
                    tracing::debug!("Updated {} RPC URL with API key", network_name);
                } else if network_config.rpc_url.contains("YOUR_API_KEY_HERE") {
                    network_config.rpc_url = network_config
                        .rpc_url
                        .replace("YOUR_API_KEY_HERE", &api_key);
                    tracing::debug!("Updated {} RPC URL with API key", network_name);
                }
            }
        } else {
            // Warn if using demo endpoints
            for (network_name, network_config) in &self.networks {
                if network_config.rpc_url.contains("/demo") {
                    tracing::warn!("Using demo RPC endpoint for {}, set ALCHEMY_API_KEY environment variable for better reliability", network_name);
                }
            }
        }

        // Check for other environment variables
        if let Ok(_etherscan_key) = std::env::var("ETHERSCAN_API_KEY") {
            tracing::debug!("ETHERSCAN_API_KEY found, will be used for ABI resolution");
            // ABI resolver will pick this up from environment
        }
    }

    /// Get default config file path
    pub fn default_config_path() -> Result<std::path::PathBuf> {
        let config_dir =
            dirs::config_dir().ok_or_else(|| anyhow!("Could not determine config directory"))?;
        Ok(config_dir.join("contract-mcp").join("config.toml"))
    }

    /// Generate a sample configuration file
    pub fn generate_sample() -> String {
        let sample_config = r#"# Contract MCP Server Configuration File
# This file configures networks, security settings, and server behavior

# Default network to use when none is specified
default_network = "ethereum"

# Network configurations
[networks.ethereum]
rpc_url = "https://eth-mainnet.g.alchemy.com/v2/YOUR_API_KEY_HERE"
chain_id = 1
explorer_url = "https://etherscan.io"

[networks.ethereum.gas]
default_gas_limit = 100000
max_gas_price = 50_000_000_000  # 50 Gwei
priority_fee = 2_000_000_000    # 2 Gwei

[networks.sepolia]
rpc_url = "https://eth-sepolia.g.alchemy.com/v2/YOUR_API_KEY_HERE"
chain_id = 11155111
explorer_url = "https://sepolia.etherscan.io"

[networks.sepolia.gas]
default_gas_limit = 100000
max_gas_price = 20_000_000_000  # 20 Gwei
priority_fee = 1_000_000_000    # 1 Gwei

[networks.polygon]
rpc_url = "https://polygon-mainnet.g.alchemy.com/v2/YOUR_API_KEY_HERE"
chain_id = 137
explorer_url = "https://polygonscan.com"

[networks.polygon.gas]
default_gas_limit = 100000
max_gas_price = 500_000_000_000  # 500 Gwei
priority_fee = 30_000_000_000    # 30 Gwei

[networks.arbitrum]
rpc_url = "https://arb-mainnet.g.alchemy.com/v2/YOUR_API_KEY_HERE"
chain_id = 42161
explorer_url = "https://arbiscan.io"

[networks.arbitrum.gas]
default_gas_limit = 100000
max_gas_price = 5_000_000_000   # 5 Gwei
priority_fee = 100_000_000      # 0.1 Gwei

# Security settings
[security]
allow_write_operations = false
require_confirmation = true
# max_transaction_value = "1000000000000000000"  # 1 ETH in wei

# Server configuration
[server]
transport = "stdio"

[server.stdio]
buffer_size = 1048576  # 1MB

# Environment variables that can be used:
# ETHERSCAN_API_KEY - Your Etherscan API key for ABI resolution
# ALCHEMY_API_KEY - Your Alchemy API key (replace YOUR_API_KEY_HERE above)
# INFURA_API_KEY - Your Infura API key (alternative to Alchemy)
"#;
        sample_config.to_string()
    }
}
