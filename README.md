# Ethereum Contract MCP Server - Setup Guide

(This was written with claude code as a test for using it for the first time)

This guide will walk you through setting up and testing the Ethereum Contract MCP Server, which provides tools for interacting with Ethereum smart contracts through the Model Context Protocol (MCP).

## 🚀 Quick Start

### Prerequisites

- **Rust**: Install from [rustup.rs](https://rustup.rs/)
- **Git**: For cloning the repository
- **API Keys** (optional but recommended):
  - Etherscan API key for ABI resolution
  - Alchemy/Infura API key for better RPC performance

### 1. Clone and Build

```bash
# Clone the repository
git clone <your-repo-url>
cd contract-mcp

# Build the project
cargo build --release

# Verify installation
cargo run -- --help
```

### 2. Generate Configuration

```bash
# See where config files should go
cargo run -- --config-path
# Output: /home/user/.config/contract-mcp/config.toml

# Generate a sample configuration
cargo run -- --generate-config > config.toml

# Or install it directly to the default location
mkdir -p ~/.config/contract-mcp
cargo run -- --generate-config > ~/.config/contract-mcp/config.toml
```

### 3. Configure API Keys (Recommended)

Edit your `config.toml` file to add your API keys:

```toml
[networks.ethereum]
rpc_url = "https://eth-mainnet.g.alchemy.com/v2/YOUR_ALCHEMY_KEY"
# ... other networks

# Or set environment variables
# export ETHERSCAN_API_KEY=your_etherscan_key
# export ALCHEMY_API_KEY=your_alchemy_key
```

### 4. Test the Server

```bash
# Start the server (it will wait for MCP protocol messages on stdin)
cargo run --release

# Or with custom config
cargo run --release -- --config ./config.toml

# Or with specific network
cargo run --release -- --network sepolia
```

## 📋 Detailed Configuration

### Configuration File Structure

The server uses TOML configuration with the following sections:

#### Networks Configuration

```toml
# Default network when none specified
default_network = "ethereum"

[networks.ethereum]
rpc_url = "https://eth-mainnet.g.alchemy.com/v2/YOUR_API_KEY"
chain_id = 1
explorer_url = "https://etherscan.io"

[networks.ethereum.gas]
default_gas_limit = 100000
max_gas_price = 50_000_000_000  # 50 Gwei
priority_fee = 2_000_000_000    # 2 Gwei
```

#### Security Settings

```toml
[security]
allow_write_operations = false      # Set to true for transaction sending
require_confirmation = true         # Always require confirmation
max_transaction_value = "1000000000000000000"  # 1 ETH in wei (optional)
```

#### Server Settings

```toml
[server]
transport = "stdio"                 # MCP transport method

[server.stdio]
buffer_size = 1048576              # 1MB buffer
```

### Environment Variables

The server recognizes these environment variables:

- `ETHERSCAN_API_KEY`: Your Etherscan API key for ABI resolution
- `ALCHEMY_API_KEY`: Your Alchemy API key (automatically replaces demo URLs and YOUR_API_KEY_HERE placeholders)
- `INFURA_API_KEY`: Your Infura API key (alternative to Alchemy)
- `RUST_LOG`: Set logging level (e.g., `debug`, `info`, `warn`, `error`)

**Note**: When `ALCHEMY_API_KEY` is set, the server automatically:

- Replaces `/demo` URLs with your API key
- Substitutes `YOUR_API_KEY_HERE` placeholders in config files
- Logs confirmation that the API key is being used

## 🔧 Testing the Server

### 1. Basic Connectivity Test

Test that the server can connect to networks:

```bash
# The server will start and wait for MCP messages
# You can test connectivity by checking the logs
RUST_LOG=info cargo run -- --network ethereum
```

### 2. Test Contract Information Retrieval

Once connected to an MCP client, you can test with popular contracts:

**USDC Contract (Ethereum Mainnet)**

- Address: `0xA0b86a33E6417c4dea4a89F56d3c9b3b89Ade32c`
- Network: `ethereum`

**Test Commands** (through MCP client):

```json
{
  "method": "tools/call",
  "params": {
    "name": "get_contract_info",
    "arguments": {
      "address": "0xA0b86a33E6417c4dea4a89F56d3c9b3b89Ade32c",
      "network": "ethereum"
    }
  }
}
```

### 3. Test Function Calls

**Call a view function (balanceOf on USDC):**

```json
{
  "method": "tools/call",
  "params": {
    "name": "call_view_function",
    "arguments": {
      "contract_address": "0xA0b86a33E6417c4dea4a89F56d3c9b3b89Ade32c",
      "function_name": "balanceOf",
      "parameters": ["0x742d35Cc6435C9c1c72c5E7b18BaB7e1DB7a5d6e"],
      "network": "ethereum"
    }
  }
}
```

### 4. Test Gas Estimation

```json
{
  "method": "tools/call",
  "params": {
    "name": "estimate_gas",
    "arguments": {
      "contract_address": "0xA0b86a33E6417c4dea4a89F56d3c9b3b89Ade32c",
      "function_name": "transfer",
      "parameters": ["0x742d35Cc6435C9c1c72c5E7b18BaB7e1DB7a5d6e", "1000000"],
      "from": "0x742d35Cc6435C9c1c72c5E7b18BaB7e1DB7a5d6e",
      "network": "ethereum"
    }
  }
}
```

## 🔍 Available Tools

The MCP server provides these tools:

### 1. `get_contract_info`

- **Purpose**: Get contract metadata, ABI, and verification status
- **Parameters**: `address`, `network` (optional)
- **Returns**: Contract information including ABI if verified

### 2. `call_view_function`

- **Purpose**: Call read-only contract functions
- **Parameters**: `contract_address`, `function_name`, `parameters`, `network` (optional)
- **Returns**: Function return value(s)

### 3. `estimate_gas`

- **Purpose**: Estimate gas cost for a transaction
- **Parameters**: `contract_address`, `function_name`, `parameters`, `from` (optional), `value` (optional), `network` (optional)
- **Returns**: Estimated gas units

### 4. `get_contract_events`

- **Purpose**: Retrieve events emitted by a contract
- **Parameters**: `contract_address`, `from_block` (optional), `to_block` (optional), `network` (optional)
- **Returns**: Array of events

### 5. `simulate_transaction`

- **Purpose**: Simulate a transaction without executing it
- **Parameters**: `contract_address`, `function_name`, `parameters`, `from` (optional), `value` (optional), `network` (optional)
- **Returns**: Simulation result with success/failure and return data

## 📊 Supported Networks

Default configuration includes:

- **Ethereum Mainnet** (`ethereum`)
- **Sepolia Testnet** (`sepolia`)
- **Polygon** (`polygon`)
- **Arbitrum** (`arbitrum`)

### Adding Custom Networks

Add new networks to your config file:

```toml
[networks.my_network]
rpc_url = "https://rpc.my-network.com"
chain_id = 12345
explorer_url = "https://explorer.my-network.com"

[networks.my_network.gas]
default_gas_limit = 100000
max_gas_price = 20_000_000_000
priority_fee = 1_000_000_000
```

## 🛠️ Troubleshooting

### Common Issues

**1. "Network validation failed"**

- Check that the network name matches your config
- Use `--network <name>` or update `default_network` in config

**2. "ABI resolution failed"**

- Contract may not be verified on Etherscan
- Add `ETHERSCAN_API_KEY` environment variable
- Check that the contract address is correct

**3. "RPC connection failed"**

- Verify your RPC URL is correct and accessible
- Check if you need API keys for the RPC provider
- Test network connectivity

**4. "Invalid contract address"**

- Ensure address is 42 characters (0x + 40 hex digits)
- Check that the address has proper checksumming

### Debug Mode

Run with detailed logging:

```bash
RUST_LOG=debug cargo run -- --config ./config.toml
```

### Configuration Validation

Test your configuration:

```bash
# Check config file location
cargo run -- --config-path

# Validate config loads properly
cargo run -- --config ./config.toml --network ethereum
# Look for "Loaded configuration from file" in logs
```

## 🔗 Integration Examples

### Claude Desktop Integration

Add to your Claude Desktop configuration:

```json
{
  "mcpServers": {
    "ethereum-contracts": {
      "command": "/path/to/contract-mcp/target/release/contract-mcp",
      "args": ["--config", "/path/to/your/config.toml"],
      "env": {
        "ETHERSCAN_API_KEY": "your-api-key-here"
      }
    }
  }
}
```

### API Key Best Practices

1. **Never commit API keys to version control**
2. **Use environment variables in production**
3. **Get free API keys from:**
   - Etherscan: https://etherscan.io/apis
   - Alchemy: https://www.alchemy.com/
   - Infura: https://infura.io/

### Rate Limiting

- **Etherscan**: 5 calls/second with free API key
- **Alchemy**: 300 requests/second on free tier
- **Infura**: 100,000 requests/day on free tier

## 🧪 Example Test Scenarios

### 1. ERC-20 Token Analysis

```bash
# Get USDC contract info
# Call balanceOf for an address
# Check totalSupply
# Estimate transfer gas cost
```

### 2. NFT Contract Interaction

```bash
# Get contract info for popular NFT (e.g., CryptoPunks)
# Call tokenURI for a specific token
# Check owner of a token
# Get contract events
```

### 3. DeFi Protocol Testing

```bash
# Interact with Uniswap contracts
# Check liquidity pool information
# Simulate swap transactions
# Estimate gas for complex DeFi operations
```

## 📝 Next Steps

After setup:

1. **Test with known contracts** using the examples above
2. **Integrate with your preferred MCP client**
3. **Configure API keys** for better performance
4. **Customize networks** for your specific needs
5. **Enable write operations** if you need transaction sending (use with caution!)

## 🤝 Support

If you encounter issues:

1. Check the troubleshooting section above
2. Enable debug logging with `RUST_LOG=debug`
3. Verify your configuration matches the examples
4. Test with well-known contracts first

---

**Happy contract interaction! 🚀**
