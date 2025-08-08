use alloy::json_abi::JsonAbi;
use anyhow::{anyhow, Result};
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;
use tracing::{debug, info, warn};

/// ABI source configuration
#[derive(Debug, Clone)]
pub struct AbiSource {
    pub etherscan_api_key: Option<String>,
    pub cache_dir: PathBuf,
}

impl Default for AbiSource {
    fn default() -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("contract-mcp")
            .join("abi-cache");

        Self {
            etherscan_api_key: std::env::var("ETHERSCAN_API_KEY").ok(),
            cache_dir,
        }
    }
}

/// ABI resolver that can fetch and cache contract ABIs
#[derive(Debug)]
pub struct AbiResolver {
    client: Client,
    config: AbiSource,
    memory_cache: HashMap<String, JsonAbi>,
}

impl AbiResolver {
    pub fn new(config: AbiSource) -> Self {
        Self {
            client: Client::new(),
            config,
            memory_cache: HashMap::new(),
        }
    }

    /// Get ABI for a contract, trying cache first, then Etherscan
    pub async fn get_abi(&mut self, address: &str, network: Option<&str>) -> Result<JsonAbi> {
        let address = address.to_lowercase();
        let cache_key = format!("{}_{}", network.unwrap_or("mainnet"), address);

        // Check memory cache first
        if let Some(abi) = self.memory_cache.get(&cache_key) {
            debug!("ABI cache hit for {}", address);
            return Ok(abi.clone());
        }

        // Check disk cache
        if let Ok(abi) = self.load_cached_abi(&cache_key).await {
            debug!("ABI disk cache hit for {}", address);
            self.memory_cache.insert(cache_key.clone(), abi.clone());
            return Ok(abi);
        }

        // Fetch from Etherscan
        info!("Fetching ABI from Etherscan for {}", address);
        let abi = self.fetch_from_etherscan(&address, network).await?;

        // Cache the result
        if let Err(e) = self.cache_abi(&cache_key, &abi).await {
            warn!("Failed to cache ABI for {}: {}", address, e);
        }

        self.memory_cache.insert(cache_key, abi.clone());
        Ok(abi)
    }

    /// Fetch ABI from Etherscan API
    async fn fetch_from_etherscan(&self, address: &str, network: Option<&str>) -> Result<JsonAbi> {
        let base_url = match network.unwrap_or("mainnet") {
            "mainnet" | "ethereum" => "https://api.etherscan.io",
            "sepolia" => "https://api-sepolia.etherscan.io",
            "goerli" => "https://api-goerli.etherscan.io",
            "polygon" => "https://api.polygonscan.com",
            "arbitrum" => "https://api.arbiscan.io",
            "optimism" => "https://api-optimistic.etherscan.io",
            other => return Err(anyhow!("Unsupported network for Etherscan: {}", other)),
        };

        let mut url = format!(
            "{}/api?module=contract&action=getabi&address={}&format=json",
            base_url, address
        );

        // Add API key if available
        if let Some(api_key) = &self.config.etherscan_api_key {
            url.push_str(&format!("&apikey={}", api_key));
        }

        let response: Value = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to fetch from Etherscan: {}", e))?
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse Etherscan response: {}", e))?;

        // Check if the response is successful
        if response["status"] != "1" {
            let message = response["message"].as_str().unwrap_or("Unknown error");
            return Err(anyhow!("Etherscan API error: {}", message));
        }

        // Parse the ABI
        let abi_str = response["result"]
            .as_str()
            .ok_or_else(|| anyhow!("No ABI found in response"))?;

        if abi_str == "Contract source code not verified" {
            return Err(anyhow!("Contract source code is not verified on Etherscan"));
        }

        let abi: JsonAbi = serde_json::from_str(abi_str)
            .map_err(|e| anyhow!("Failed to parse ABI JSON: {}", e))?;

        Ok(abi)
    }

    /// Load ABI from disk cache
    async fn load_cached_abi(&self, cache_key: &str) -> Result<JsonAbi> {
        let cache_path = self.config.cache_dir.join(format!("{}.json", cache_key));

        if !cache_path.exists() {
            return Err(anyhow!("Cache file does not exist"));
        }

        let content = fs::read_to_string(&cache_path)
            .await
            .map_err(|e| anyhow!("Failed to read cache file: {}", e))?;

        let abi: JsonAbi = serde_json::from_str(&content)
            .map_err(|e| anyhow!("Failed to parse cached ABI: {}", e))?;

        Ok(abi)
    }

    /// Save ABI to disk cache
    async fn cache_abi(&self, cache_key: &str, abi: &JsonAbi) -> Result<()> {
        // Create cache directory if it doesn't exist
        if !self.config.cache_dir.exists() {
            fs::create_dir_all(&self.config.cache_dir)
                .await
                .map_err(|e| anyhow!("Failed to create cache directory: {}", e))?;
        }

        let cache_path = self.config.cache_dir.join(format!("{}.json", cache_key));
        let content = serde_json::to_string_pretty(abi)
            .map_err(|e| anyhow!("Failed to serialize ABI: {}", e))?;

        fs::write(&cache_path, content)
            .await
            .map_err(|e| anyhow!("Failed to write cache file: {}", e))?;

        debug!("Cached ABI to {:?}", cache_path);
        Ok(())
    }

    /// Add ABI manually (for unverified contracts)
    pub fn add_manual_abi(&mut self, address: &str, network: Option<&str>, abi: JsonAbi) {
        let cache_key = format!(
            "{}_{}",
            network.unwrap_or("mainnet"),
            address.to_lowercase()
        );
        self.memory_cache.insert(cache_key, abi);
        info!("Added manual ABI for {}", address);
    }

    /// Check if we have an ABI for a contract (without fetching)
    pub async fn has_abi(&self, address: &str, network: Option<&str>) -> bool {
        let cache_key = format!(
            "{}_{}",
            network.unwrap_or("mainnet"),
            address.to_lowercase()
        );

        // Check memory cache
        if self.memory_cache.contains_key(&cache_key) {
            return true;
        }

        // Check disk cache
        let cache_path = self.config.cache_dir.join(format!("{}.json", cache_key));
        cache_path.exists()
    }

    /// Clear all cached ABIs
    pub async fn clear_cache(&mut self) -> Result<()> {
        self.memory_cache.clear();

        if self.config.cache_dir.exists() {
            fs::remove_dir_all(&self.config.cache_dir)
                .await
                .map_err(|e| anyhow!("Failed to clear cache directory: {}", e))?;
        }

        info!("Cleared ABI cache");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_abi_resolver_creation() {
        let temp_dir = tempdir().unwrap();
        let config = AbiSource {
            etherscan_api_key: None,
            cache_dir: temp_dir.path().to_path_buf(),
        };

        let resolver = AbiResolver::new(config);
        assert!(resolver.memory_cache.is_empty());
    }

    #[tokio::test]
    async fn test_manual_abi_addition() {
        let temp_dir = tempdir().unwrap();
        let config = AbiSource {
            etherscan_api_key: None,
            cache_dir: temp_dir.path().to_path_buf(),
        };

        let mut resolver = AbiResolver::new(config);
        let test_abi: JsonAbi = serde_json::from_str("[]").unwrap();

        resolver.add_manual_abi("0x123", Some("mainnet"), test_abi.clone());

        assert!(resolver.has_abi("0x123", Some("mainnet")).await);
        let retrieved_abi = resolver.get_abi("0x123", Some("mainnet")).await.unwrap();
        assert_eq!(
            retrieved_abi.functions().count(),
            test_abi.functions().count()
        );
    }
}
