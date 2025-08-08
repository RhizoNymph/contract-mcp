use alloy::primitives::Address;
use anyhow::{anyhow, Result};
use std::str::FromStr;

/// Validates and normalizes an Ethereum address
pub fn validate_address(address: &str) -> Result<Address> {
    let address = address.trim();

    if address.is_empty() {
        return Err(anyhow!("Address cannot be empty"));
    }

    if !address.starts_with("0x") && !address.starts_with("0X") {
        return Err(anyhow!(
            "Invalid address format: '{}'. Ethereum addresses must start with '0x'",
            address
        ));
    }

    if address.len() != 42 {
        return Err(anyhow!(
            "Invalid address length: '{}'. Ethereum addresses must be exactly 42 characters (0x + 40 hex characters)",
            address
        ));
    }

    // Check if all characters after 0x are valid hex
    let hex_part = &address[2..];
    if !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(anyhow!(
            "Invalid address format: '{}'. Contains non-hexadecimal characters",
            address
        ));
    }

    // Parse using Alloy's Address type which handles checksumming
    Address::from_str(address)
        .map_err(|e| anyhow!("Invalid Ethereum address: '{}'. Error: {}", address, e))
}

/// Validates network name
pub fn validate_network(network: &str, available_networks: &[String]) -> Result<()> {
    if network.is_empty() {
        return Err(anyhow!("Network name cannot be empty"));
    }

    if !available_networks.contains(&network.to_string()) {
        return Err(anyhow!(
            "Unknown network: '{}'. Available networks: {}",
            network,
            available_networks.join(", ")
        ));
    }

    Ok(())
}

/// Validates function name
pub fn validate_function_name(function_name: &str) -> Result<()> {
    if function_name.is_empty() {
        return Err(anyhow!("Function name cannot be empty"));
    }

    // Check for valid Solidity identifier
    if !function_name.chars().next().unwrap().is_ascii_alphabetic()
        && !function_name.starts_with('_')
    {
        return Err(anyhow!(
            "Invalid function name: '{}'. Function names must start with a letter or underscore",
            function_name
        ));
    }

    if !function_name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(anyhow!(
            "Invalid function name: '{}'. Function names can only contain letters, numbers, and underscores",
            function_name
        ));
    }

    Ok(())
}

/// Validates and parses a hex string value (for transaction values)
pub fn validate_hex_value(value_str: &str) -> Result<alloy::primitives::U256> {
    if value_str.is_empty() {
        return Err(anyhow!("Value cannot be empty"));
    }

    let value = if value_str.starts_with("0x") || value_str.starts_with("0X") {
        alloy::primitives::U256::from_str_radix(&value_str[2..], 16)
            .map_err(|_| anyhow!("Invalid hexadecimal value: '{}'", value_str))?
    } else {
        // Try parsing as decimal first
        alloy::primitives::U256::from_str(value_str).map_err(|_| {
            anyhow!(
                "Invalid numeric value: '{}'. Use decimal format or '0x' prefixed hex",
                value_str
            )
        })?
    };

    Ok(value)
}

/// Validates block number
pub fn validate_block_number(block: Option<u64>) -> Result<u64> {
    match block {
        Some(b) if b > u64::MAX / 2 => Err(anyhow!(
            "Block number {} is too large. Maximum supported block number is {}",
            b,
            u64::MAX / 2
        )),
        Some(b) => Ok(b),
        None => Ok(0),
    }
}

/// Creates user-friendly error messages for common RPC errors
pub fn interpret_rpc_error(error: &str) -> String {
    if error.contains("execution reverted") {
        format!(
            "Transaction failed: The contract function reverted execution. {}",
            if error.contains("revert") {
                "This usually means the function's requirements were not met or an assertion failed."
            } else {
                "Check your parameters and try again."
            }
        )
    } else if error.contains("insufficient funds") {
        "Transaction failed: Insufficient funds to cover gas costs. Make sure your account has enough ETH for gas fees.".to_string()
    } else if error.contains("gas required exceeds allowance") {
        "Transaction failed: Gas limit too low. Try increasing the gas limit for this transaction."
            .to_string()
    } else if error.contains("nonce too low") {
        "Transaction failed: Nonce too low. This usually means another transaction was already mined with this nonce.".to_string()
    } else if error.contains("replacement transaction underpriced") {
        "Transaction failed: Gas price too low to replace pending transaction. Increase the gas price.".to_string()
    } else if error.contains("connection refused") || error.contains("network unreachable") {
        "Network error: Cannot connect to RPC endpoint. Check your internet connection and RPC URL configuration.".to_string()
    } else if error.contains("timeout") {
        "Network error: Request timed out. The RPC endpoint may be overloaded or unreachable."
            .to_string()
    } else if error.contains("rate limit") {
        "Rate limit error: Too many requests to the RPC endpoint. Try again in a few moments or use a different endpoint.".to_string()
    } else if error.contains("method not found") {
        "RPC error: The requested method is not supported by this RPC endpoint. Try using a different endpoint.".to_string()
    } else {
        format!("RPC error: {}", error)
    }
}

/// Creates user-friendly error messages for ABI-related errors
pub fn interpret_abi_error(error: &str, contract_address: &str) -> String {
    if error.contains("404") || error.contains("not found") {
        format!(
            "Contract verification not found: The contract at {} is not verified on Etherscan. Verified contracts are required for automatic ABI resolution.",
            contract_address
        )
    } else if error.contains("rate limit") || error.contains("429") {
        "API rate limit: Too many requests to Etherscan API. Try again in a few moments or provide your own ETHERSCAN_API_KEY.".to_string()
    } else if error.contains("invalid API key") || error.contains("403") {
        "API authentication error: Invalid Etherscan API key. Check your ETHERSCAN_API_KEY environment variable.".to_string()
    } else if error.contains("network") || error.contains("connection") {
        "Network error: Cannot connect to Etherscan API. Check your internet connection."
            .to_string()
    } else if error.contains("timeout") {
        "Timeout error: Request to Etherscan API timed out. Try again in a few moments.".to_string()
    } else {
        format!("ABI resolution error: {}", error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_address() {
        // Valid addresses
        assert!(validate_address("0x742d35Cc6435C9c1c72c5E7b18BaB7e1DB7a5d6e").is_ok());
        assert!(validate_address("0x0000000000000000000000000000000000000000").is_ok());

        // Invalid addresses
        assert!(validate_address("").is_err());
        assert!(validate_address("not_an_address").is_err());
        assert!(validate_address("0x123").is_err()); // Too short
        assert!(validate_address("742d35Cc6435C9c1c72c5E7b18BaB7e1DB7a5d6e").is_err()); // Missing 0x
        assert!(validate_address("0xgg2d35Cc6435C9c1c72c5E7b18BaB7e1DB7a5d6e").is_err());
        // Invalid hex
    }

    #[test]
    fn test_validate_network() {
        let networks = vec!["ethereum".to_string(), "sepolia".to_string()];

        assert!(validate_network("ethereum", &networks).is_ok());
        assert!(validate_network("sepolia", &networks).is_ok());
        assert!(validate_network("invalid", &networks).is_err());
        assert!(validate_network("", &networks).is_err());
    }

    #[test]
    fn test_validate_function_name() {
        assert!(validate_function_name("transfer").is_ok());
        assert!(validate_function_name("_internal").is_ok());
        assert!(validate_function_name("getBalance123").is_ok());

        assert!(validate_function_name("").is_err());
        assert!(validate_function_name("123invalid").is_err());
        assert!(validate_function_name("invalid-name").is_err());
    }
}
