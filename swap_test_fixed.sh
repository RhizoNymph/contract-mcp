#!/bin/bash

# Set up environment
source "$HOME/.cargo/env"

# MCP server executable
SERVER="./target/release/contract-mcp"

# Function to call MCP server and get response
call_mcp() {
    local request="$1"
    local expected_id="$2"
    
    # Use a simpler approach with timeout and proper stream handling
    timeout 30 bash -c "
        (echo '{\"jsonrpc\": \"2.0\", \"method\": \"initialize\", \"params\": {\"protocolVersion\": \"2024-11-05\", \"capabilities\": {}, \"clientInfo\": {\"name\": \"swap_test\", \"version\": \"1.0\"}}, \"id\": 1}';
         echo '{\"jsonrpc\": \"2.0\", \"method\": \"notifications/initialized\", \"params\": {}}';
         echo '$request';
         sleep 10) | $SERVER --network ethereum --allow-writes
    " | grep "\"id\":$expected_id" | tail -1
}

# Contract addresses
weth_address="0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
usdc_address="0xA0b86a33E6441E1Bb76a85d6e0d945C1E87e1c00" 
uniswap_router="0xE592427A0AEce92De3Edee1F18E0157C05861564"
wallet_address="0xfe723c1caf93386b4fed11b76e58a28f0a0aabe7"  # This is the actual sender address with WETH

# Get current timestamp and calculate deadline
current_time=$(date +%s)
deadline=$((current_time + 1200))

echo "=== Starting Uniswap Swap Test ==="
echo "Wallet: $wallet_address"
echo "Deadline: $deadline"
echo ""

# Step 1: Check WETH balance
echo "Step 1: Checking WETH balance..."
balance_request="{\"jsonrpc\": \"2.0\", \"method\": \"tools/call\", \"params\": {\"name\": \"call_view_function\", \"arguments\": {\"contract_address\": \"$weth_address\", \"function_name\": \"balanceOf\", \"parameters\": [\"$wallet_address\"]}}, \"id\": 10}"
balance_result=$(call_mcp "$balance_request" 10)
echo "WETH Balance result: $balance_result"

# Extract balance from result (this is a simplified check - in production you'd parse the JSON properly)
if [[ "$balance_result" == *"\"result\": \"0\""* ]]; then
    echo "No WETH found. Need to deposit ETH first."
    echo "Step 1a: Depositing ETH to get WETH..."
    deposit_request="{\"jsonrpc\": \"2.0\", \"method\": \"tools/call\", \"params\": {\"name\": \"send_transaction\", \"arguments\": {\"contract_address\": \"$weth_address\", \"function_name\": \"deposit\", \"parameters\": [], \"value\": \"10000000000000000\", \"gas_limit\": 50000}}, \"id\": 11}"
    deposit_result=$(call_mcp "$deposit_request" 11)
    echo "Deposit result: $deposit_result"
else
    echo "WETH balance found, skipping deposit."
fi
echo ""

# Step 2: Check current allowance
echo "Step 2: Checking WETH allowance for Uniswap router..."
allowance_request="{\"jsonrpc\": \"2.0\", \"method\": \"tools/call\", \"params\": {\"name\": \"call_view_function\", \"arguments\": {\"contract_address\": \"$weth_address\", \"function_name\": \"allowance\", \"parameters\": [\"$wallet_address\", \"$uniswap_router\"]}}, \"id\": 12}"
allowance_result=$(call_mcp "$allowance_request" 12)
echo "Allowance result: $allowance_result"

# Check if allowance is sufficient (only approve if result is exactly "0")
if [[ "$allowance_result" == *'"result": "0"'* ]]; then
    echo "No allowance found (result: 0). Need to approve WETH spending."
    echo "Step 2a: Approving WETH spending..."
    approval_request="{\"jsonrpc\": \"2.0\", \"method\": \"tools/call\", \"params\": {\"name\": \"send_transaction\", \"arguments\": {\"contract_address\": \"$weth_address\", \"function_name\": \"approve\", \"parameters\": [\"$uniswap_router\", \"20000000000000000\"], \"gas_limit\": 60000}}, \"id\": 2}"
    approval_result=$(call_mcp "$approval_request" 2)
    echo "Approval result: $approval_result"
elif [[ "$allowance_result" == *"error"* ]]; then
    echo "Error checking allowance. Attempting to approve anyway..."
    echo "Step 2a: Approving WETH spending..."
    approval_request="{\"jsonrpc\": \"2.0\", \"method\": \"tools/call\", \"params\": {\"name\": \"send_transaction\", \"arguments\": {\"contract_address\": \"$weth_address\", \"function_name\": \"approve\", \"parameters\": [\"$uniswap_router\", \"20000000000000000\"], \"gas_limit\": 60000}}, \"id\": 2}"
    approval_result=$(call_mcp "$approval_request" 2)
    echo "Approval result: $approval_result"
else
    # Extract just to display the value 
    allowance_display=$(echo "$allowance_result" | grep -o '"result": "[^"]*"' | cut -d'"' -f4)
    echo "Current allowance: $allowance_display wei (sufficient for swap)"
    echo "Sufficient allowance found, skipping approval."
fi
echo ""

# Step 3: Execute the swap (swap all WETH for whatever USDC we can get)
echo "Step 3: Executing Uniswap swap..."
swap_request="{\"jsonrpc\": \"2.0\", \"method\": \"tools/call\", \"params\": {\"name\": \"send_transaction\", \"arguments\": {\"contract_address\": \"$uniswap_router\", \"function_name\": \"exactInputSingle\", \"parameters\": [{\"params\": [\"$weth_address\", \"$usdc_address\", 500, \"$wallet_address\", $deadline, \"10000000000000000\", \"1\", \"0\"]}], \"gas_limit\": 400000}}, \"id\": 3}"
swap_result=$(call_mcp "$swap_request" 3)
echo "Swap result: $swap_result"
echo ""

echo "=== Swap test completed ==="