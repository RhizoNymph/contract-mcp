use crate::config::{Config, NetworkConfig};
use alloy::{
    providers::{Provider, ProviderBuilder, RootProvider},
    transports::http::{Client, Http},
};
use anyhow::{anyhow, Result};
use std::collections::HashMap;

#[derive(Debug)]
pub struct ProviderManager {
    providers: HashMap<String, RootProvider<Http<Client>>>,
    config: Config,
}

impl ProviderManager {
    pub fn new(config: Config) -> Result<Self> {
        let mut providers = HashMap::new();

        for (network_name, network_config) in &config.networks {
            let provider = Self::create_provider(network_config)?;
            providers.insert(network_name.clone(), provider);
        }

        Ok(Self { providers, config })
    }

    fn create_provider(network_config: &NetworkConfig) -> Result<RootProvider<Http<Client>>> {
        let provider = ProviderBuilder::new().on_http(network_config.rpc_url.parse()?);

        Ok(provider)
    }

    pub fn get_provider(&self, network: Option<&str>) -> Result<&RootProvider<Http<Client>>> {
        let network_name = network.unwrap_or(&self.config.default_network);
        self.providers
            .get(network_name)
            .ok_or_else(|| anyhow!("Network '{}' not found", network_name))
    }

    #[allow(dead_code)]
    pub fn get_network_config(&self, network: Option<&str>) -> Result<&NetworkConfig> {
        let network_name = network.unwrap_or(&self.config.default_network);
        self.config
            .networks
            .get(network_name)
            .ok_or_else(|| anyhow!("Network '{}' not configured", network_name))
    }

    #[allow(dead_code)]
    pub fn list_networks(&self) -> Vec<&String> {
        self.config.networks.keys().collect()
    }

    #[allow(dead_code)]
    pub async fn check_connection(&self, network: Option<&str>) -> Result<bool> {
        let provider = self.get_provider(network)?;
        match provider.get_block_number().await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    #[allow(dead_code)]
    pub async fn get_chain_id(&self, network: Option<&str>) -> Result<u64> {
        let provider = self.get_provider(network)?;
        let chain_id = provider.get_chain_id().await?;
        Ok(chain_id)
    }
}
