use alloy::{
    dyn_abi::{DynSolValue, FunctionExt, JsonAbiExt, Word},
    primitives::{Address, Bytes, U256},
    providers::Provider,
    rpc::types::{Filter, TransactionRequest},
};
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::str::FromStr;

use super::{CallResult, ContractInfo, EventInfo, FunctionCall, TransactionInfo};
use crate::ethereum::{abi::AbiResolver, provider::ProviderManager, utils};

#[derive(Debug)]
pub struct ContractManager {
    provider_manager: ProviderManager,
    abi_resolver: AbiResolver,
}

impl ContractManager {
    pub fn new(provider_manager: ProviderManager) -> Self {
        use crate::ethereum::abi::AbiSource;
        let abi_resolver = AbiResolver::new(AbiSource::default());
        Self {
            provider_manager,
            abi_resolver,
        }
    }

    pub async fn get_contract_info(
        &mut self,
        address: &str,
        network: Option<&str>,
    ) -> Result<ContractInfo> {
        // Validate the contract address
        let contract_address = utils::validate_address(address)
            .map_err(|e| anyhow!("Invalid contract address: {}", e))?;

        // Validate network if provided
        if let Some(net) = network {
            let available_networks: Vec<String> = self.provider_manager.get_available_networks();
            utils::validate_network(net, &available_networks)
                .map_err(|e| anyhow!("Network validation failed: {}", e))?;
        }

        let provider = self.provider_manager.get_provider(network).map_err(|e| {
            anyhow!(
                "Failed to get provider for network '{}': {}",
                network.unwrap_or("default"),
                e
            )
        })?;

        tracing::debug!("Fetching bytecode for contract: {:?}", contract_address);
        let bytecode = provider.get_code_at(contract_address).await.map_err(|e| {
            tracing::error!("RPC error details: {}", e);
            anyhow!(
                "Failed to fetch contract bytecode: {}",
                utils::interpret_rpc_error(&e.to_string())
            )
        })?;

        // Check if contract exists (has bytecode)
        if bytecode.is_empty() {
            return Err(anyhow!(
                "No contract found at address '{}' on network '{}'. The address may be incorrect, or the contract may not be deployed yet.",
                address, network.unwrap_or("default")
            ));
        }

        // Try to get ABI from Etherscan
        let (abi_value, verified) = match self.abi_resolver.get_abi(address, network).await {
            Ok(abi) => {
                let abi_value =
                    serde_json::to_value(&abi).unwrap_or_else(|_| serde_json::json!([]));
                (abi_value, true)
            }
            Err(e) => {
                let friendly_error = utils::interpret_abi_error(&e.to_string(), address);
                tracing::debug!("ABI resolution failed for {}: {}", address, friendly_error);
                (serde_json::json!([]), false)
            }
        };

        let info = ContractInfo {
            address: format!("{:?}", contract_address), // This gives us the checksummed address
            name: None, // Could be extracted from ABI or contract name resolution
            abi: abi_value,
            bytecode: if bytecode.is_empty() {
                None
            } else {
                Some(format!("0x{}", hex::encode(&bytecode)))
            },
            deployment_block: None, // Would need to search for contract creation
            creator: None,          // Would need creation transaction analysis
            verified,
        };

        Ok(info)
    }

    pub async fn call_view_function(
        &mut self,
        contract_address: &str,
        function_call: &FunctionCall,
        network: Option<&str>,
    ) -> Result<CallResult> {
        // Validate inputs
        let address = utils::validate_address(contract_address)
            .map_err(|e| anyhow!("Invalid contract address: {}", e))?;

        utils::validate_function_name(&function_call.function_name)
            .map_err(|e| anyhow!("Invalid function name: {}", e))?;

        if let Some(net) = network {
            let available_networks = self.provider_manager.get_available_networks();
            utils::validate_network(net, &available_networks)
                .map_err(|e| anyhow!("Network validation failed: {}", e))?;
        }

        let provider = self
            .provider_manager
            .get_provider(network)
            .map_err(|e| anyhow!("Failed to get provider: {}", e))?;

        // Get the ABI for the contract
        let abi = match self.abi_resolver.get_abi(contract_address, network).await {
            Ok(abi) => abi,
            Err(e) => {
                return Ok(CallResult {
                    success: false,
                    result: None,
                    error: Some(utils::interpret_abi_error(&e.to_string(), contract_address)),
                    gas_used: None,
                    transaction_hash: None,
                });
            }
        };

        // Find the function in the ABI
        let function = abi
            .functions()
            .find(|f| f.name == function_call.function_name)
            .ok_or_else(|| {
                let available_functions: Vec<String> =
                    abi.functions().map(|f| f.name.clone()).collect();

                if available_functions.is_empty() {
                    anyhow!(
                        "Function '{}' not found. The contract ABI contains no functions.",
                        function_call.function_name
                    )
                } else {
                    anyhow!(
                        "Function '{}' not found in contract ABI. Available functions: {}",
                        function_call.function_name,
                        available_functions.join(", ")
                    )
                }
            })?;

        // Encode the function call
        let calldata = match self.encode_function_call(function, &function_call.parameters) {
            Ok(data) => data,
            Err(e) => {
                return Ok(CallResult {
                    success: false,
                    result: None,
                    error: Some(format!("Failed to encode function call: {}", e)),
                    gas_used: None,
                    transaction_hash: None,
                });
            }
        };

        // Make the eth_call
        let call_request = TransactionRequest::default()
            .to(address)
            .input(calldata.into());

        match provider.call(&call_request).await {
            Ok(result_bytes) => {
                // Decode the result
                match self.decode_function_result(function, &result_bytes) {
                    Ok(decoded) => Ok(CallResult {
                        success: true,
                        result: Some(decoded),
                        error: None,
                        gas_used: None,
                        transaction_hash: None,
                    }),
                    Err(e) => Ok(CallResult {
                        success: false,
                        result: Some(serde_json::json!({
                            "raw_result": format!("0x{}", hex::encode(&result_bytes)),
                            "decode_error": e.to_string()
                        })),
                        error: Some(format!("Failed to decode result: {}", e)),
                        gas_used: None,
                        transaction_hash: None,
                    }),
                }
            }
            Err(e) => Ok(CallResult {
                success: false,
                result: None,
                error: Some(utils::interpret_rpc_error(&e.to_string())),
                gas_used: None,
                transaction_hash: None,
            }),
        }
    }

    /// Encode function parameters for a contract call
    fn encode_function_call(
        &self,
        function: &alloy::json_abi::Function,
        parameters: &Value,
    ) -> Result<Bytes> {
        // Convert JSON parameters to DynSolValue
        let inputs = match parameters {
            Value::Array(params) => {
                if params.len() != function.inputs.len() {
                    let expected_params: Vec<String> = function
                        .inputs
                        .iter()
                        .map(|input| format!("{} {}", input.ty, input.name))
                        .collect();

                    return Err(anyhow!(
                        "Parameter count mismatch for function '{}': expected {} parameters, got {}.\nExpected parameters: [{}]",
                        function.name,
                        function.inputs.len(),
                        params.len(),
                        expected_params.join(", ")
                    ));
                }

                let mut dyn_values = Vec::new();
                for (i, param_value) in params.iter().enumerate() {
                    let expected_type = &function.inputs[i].ty;
                    let param_name = &function.inputs[i].name;
                    let dyn_value = self
                        .json_to_dyn_sol_value(param_value, expected_type)
                        .map_err(|e| {
                            anyhow!(
                                "Invalid parameter #{} ('{}' of type '{}'): {}",
                                i + 1,
                                param_name,
                                expected_type,
                                e
                            )
                        })?;
                    dyn_values.push(dyn_value);
                }
                dyn_values
            }
            Value::Object(obj) => {
                // Named parameters
                let mut dyn_values = Vec::new();
                let expected_params: Vec<String> = function
                    .inputs
                    .iter()
                    .map(|input| format!("{}: {}", input.name, input.ty))
                    .collect();

                for input in &function.inputs {
                    let param_value = obj
                        .get(&input.name)
                        .ok_or_else(|| anyhow!(
                            "Missing required parameter '{}' of type '{}' for function '{}'.\nExpected parameters: {{{}}}",
                            input.name, input.ty, function.name, expected_params.join(", ")
                        ))?;
                    let dyn_value =
                        self.json_to_dyn_sol_value(param_value, &input.ty)
                            .map_err(|e| {
                                anyhow!(
                                    "Invalid parameter '{}' of type '{}': {}",
                                    input.name,
                                    input.ty,
                                    e
                                )
                            })?;
                    dyn_values.push(dyn_value);
                }
                dyn_values
            }
            _ => {
                let expected_params: Vec<String> = function
                    .inputs
                    .iter()
                    .map(|input| format!("{}: {}", input.name, input.ty))
                    .collect();
                return Err(anyhow!(
                    "Invalid parameter format for function '{}'. Parameters must be provided as either:\n1. Array: [value1, value2, ...]\n2. Object: {{{}}}\nProvided: {}",
                    function.name,
                    expected_params.join(", "),
                    serde_json::to_string(parameters).unwrap_or_else(|_| "invalid JSON".to_string())
                ));
            }
        };

        // Encode the function call
        let encoded = function
            .abi_encode_input(&inputs)
            .map_err(|e| anyhow!("Failed to encode function inputs: {}", e))?;

        Ok(encoded.into())
    }

    /// Decode function call result
    fn decode_function_result(
        &self,
        function: &alloy::json_abi::Function,
        result_bytes: &Bytes,
    ) -> Result<Value> {
        if result_bytes.is_empty() {
            return Ok(Value::Null);
        }

        let decoded = function
            .abi_decode_output(result_bytes, false)
            .map_err(|e| anyhow!("Failed to decode output: {}", e))?;

        // Convert DynSolValue to JSON
        self.dyn_sol_values_to_json(&decoded)
    }

    /// Convert JSON value to DynSolValue based on expected Solidity type
    fn json_to_dyn_sol_value(&self, value: &Value, sol_type: &str) -> Result<DynSolValue> {
        match sol_type {
            "address" => {
                let addr_str = value
                    .as_str()
                    .ok_or_else(|| anyhow!("Address must be a string"))?;
                let address = Address::from_str(addr_str)?;
                Ok(DynSolValue::Address(address))
            }
            ty if ty.starts_with("uint") => {
                let num = match value {
                    Value::Number(n) => {
                        if let Some(u) = n.as_u64() {
                            U256::from(u)
                        } else {
                            return Err(anyhow!("Invalid uint value"));
                        }
                    }
                    Value::String(s) => U256::from_str_radix(s.trim_start_matches("0x"), 16)
                        .or_else(|_| U256::from_str(s))
                        .map_err(|_| anyhow!("Invalid uint string: {}", s))?,
                    _ => return Err(anyhow!("Uint must be a number or string")),
                };
                Ok(DynSolValue::Uint(num, 256))
            }
            "string" => {
                let s = value
                    .as_str()
                    .ok_or_else(|| anyhow!("String parameter must be a string"))?;
                Ok(DynSolValue::String(s.to_string()))
            }
            "bool" => {
                let b = value
                    .as_bool()
                    .ok_or_else(|| anyhow!("Bool parameter must be a boolean"))?;
                Ok(DynSolValue::Bool(b))
            }
            ty if ty.starts_with("bytes") && ty != "bytes" => {
                // Fixed bytes (e.g., bytes32)
                let hex_str = value
                    .as_str()
                    .ok_or_else(|| anyhow!("Bytes must be a hex string"))?;
                let bytes = hex::decode(hex_str.trim_start_matches("0x"))
                    .map_err(|_| anyhow!("Invalid hex string: {}", hex_str))?;

                // Convert to Word (FixedBytes<32>) by padding or truncating
                let mut word_bytes = [0u8; 32];
                let len = bytes.len().min(32);
                word_bytes[..len].copy_from_slice(&bytes[..len]);
                let word = Word::from(word_bytes);

                Ok(DynSolValue::FixedBytes(word, len))
            }
            "bytes" => {
                // Dynamic bytes
                let hex_str = value
                    .as_str()
                    .ok_or_else(|| anyhow!("Bytes must be a hex string"))?;
                let bytes = hex::decode(hex_str.trim_start_matches("0x"))
                    .map_err(|_| anyhow!("Invalid hex string: {}", hex_str))?;
                Ok(DynSolValue::Bytes(bytes))
            }
            ty if ty.ends_with("[]") => {
                // Array type
                let array = value
                    .as_array()
                    .ok_or_else(|| anyhow!("Array parameter must be an array"))?;
                let element_type = &ty[..ty.len() - 2];
                let mut dyn_array = Vec::new();
                for element in array {
                    dyn_array.push(self.json_to_dyn_sol_value(element, element_type)?);
                }
                Ok(DynSolValue::Array(dyn_array))
            }
            _ => Err(anyhow!("Unsupported Solidity type: {}", sol_type)),
        }
    }

    /// Convert DynSolValue array to JSON
    fn dyn_sol_values_to_json(&self, values: &[DynSolValue]) -> Result<Value> {
        if values.len() == 1 {
            // Single return value
            self.dyn_sol_value_to_json(&values[0])
        } else {
            // Multiple return values - return as array
            let mut result = Vec::new();
            for value in values {
                result.push(self.dyn_sol_value_to_json(value)?);
            }
            Ok(Value::Array(result))
        }
    }

    /// Convert single DynSolValue to JSON
    fn dyn_sol_value_to_json(&self, value: &DynSolValue) -> Result<Value> {
        match value {
            DynSolValue::Address(addr) => Ok(Value::String(format!("0x{:x}", addr))),
            DynSolValue::Uint(num, _) => Ok(Value::String(num.to_string())),
            DynSolValue::Int(num, _) => Ok(Value::String(num.to_string())),
            DynSolValue::Bool(b) => Ok(Value::Bool(*b)),
            DynSolValue::String(s) => Ok(Value::String(s.clone())),
            DynSolValue::Bytes(bytes) => Ok(Value::String(format!("0x{}", hex::encode(bytes)))),
            DynSolValue::FixedBytes(bytes, _) => {
                Ok(Value::String(format!("0x{}", hex::encode(bytes))))
            }
            DynSolValue::Array(arr) => {
                let mut json_arr = Vec::new();
                for item in arr {
                    json_arr.push(self.dyn_sol_value_to_json(item)?);
                }
                Ok(Value::Array(json_arr))
            }
            DynSolValue::Tuple(tuple) => {
                let mut json_arr = Vec::new();
                for item in tuple {
                    json_arr.push(self.dyn_sol_value_to_json(item)?);
                }
                Ok(Value::Array(json_arr))
            }
            _ => Err(anyhow!("Unsupported DynSolValue type: {:?}", value)),
        }
    }

    pub async fn estimate_gas(
        &mut self,
        contract_address: &str,
        function_call: &FunctionCall,
        network: Option<&str>,
    ) -> Result<u64> {
        // Validate inputs
        let address = utils::validate_address(contract_address)
            .map_err(|e| anyhow!("Invalid contract address for gas estimation: {}", e))?;

        if let Some(net) = network {
            let available_networks = self.provider_manager.get_available_networks();
            utils::validate_network(net, &available_networks)
                .map_err(|e| anyhow!("Network validation failed: {}", e))?;
        }

        let provider = self
            .provider_manager
            .get_provider(network)
            .map_err(|e| anyhow!("Failed to get provider: {}", e))?;

        // If it's a simple ETH transfer (no function call), return base cost
        if function_call.function_name.is_empty() {
            return Ok(21000);
        }

        utils::validate_function_name(&function_call.function_name)
            .map_err(|e| anyhow!("Invalid function name: {}", e))?;

        // Get the ABI and encode the function call
        let abi = self
            .abi_resolver
            .get_abi(contract_address, network)
            .await
            .map_err(|e| {
                anyhow!(
                    "Could not resolve ABI for gas estimation: {}",
                    utils::interpret_abi_error(&e.to_string(), contract_address)
                )
            })?;

        let function = abi
            .functions()
            .find(|f| f.name == function_call.function_name)
            .ok_or_else(|| {
                let available_functions: Vec<String> = abi
                    .functions()
                    .map(|f| f.name.clone())
                    .collect();
                anyhow!("Function '{}' not found in contract ABI for gas estimation. Available functions: {}",
                    function_call.function_name, available_functions.join(", "))
            })?;

        let calldata = self
            .encode_function_call(function, &function_call.parameters)
            .map_err(|e| anyhow!("Failed to encode function call for gas estimation: {}", e))?;

        // Build transaction request for gas estimation
        let mut tx_request = TransactionRequest::default()
            .to(address)
            .input(calldata.into());

        // Set from address if provided
        if let Some(from_str) = &function_call.from {
            let from_address = utils::validate_address(from_str)
                .map_err(|e| anyhow!("Invalid 'from' address: {}", e))?;
            tx_request = tx_request.from(from_address);
        }

        // Set value if provided
        if let Some(value_str) = &function_call.value {
            let value = utils::validate_hex_value(value_str)
                .map_err(|e| anyhow!("Invalid transaction value: {}", e))?;
            tx_request = tx_request.value(value);
        }

        // Estimate gas
        let gas_estimate = provider.estimate_gas(&tx_request).await.map_err(|e| {
            anyhow!(
                "Gas estimation failed: {}",
                utils::interpret_rpc_error(&e.to_string())
            )
        })?;

        Ok(gas_estimate)
    }

    pub async fn get_contract_events(
        &self,
        contract_address: &str,
        from_block: Option<u64>,
        to_block: Option<u64>,
        network: Option<&str>,
    ) -> Result<Vec<EventInfo>> {
        let provider = self.provider_manager.get_provider(network)?;
        let address = Address::from_str(contract_address)?;

        let filter = Filter::new()
            .address(address)
            .from_block(from_block.unwrap_or(0))
            .to_block(to_block.unwrap_or(u64::MAX));

        let logs = provider.get_logs(&filter).await?;

        let events: Vec<EventInfo> = logs
            .into_iter()
            .enumerate()
            .map(|(index, log)| EventInfo {
                address: format!("0x{:x}", log.address()),
                topics: log.topics().iter().map(|t| format!("0x{:x}", t)).collect(),
                data: format!("0x{}", hex::encode(log.data().data.clone())),
                block_number: log.block_number.unwrap_or_default(),
                transaction_hash: format!("0x{:x}", log.transaction_hash.unwrap_or_default()),
                log_index: index as u64,
                decoded: None, // Would need ABI to decode
            })
            .collect();

        Ok(events)
    }

    #[allow(dead_code)]
    pub async fn get_transaction_history(
        &self,
        _contract_address: &str,
        _limit: Option<usize>,
        _network: Option<&str>,
    ) -> Result<Vec<TransactionInfo>> {
        // This would require indexing service integration
        // For now, return empty list
        Ok(vec![])
    }

    pub async fn simulate_transaction(
        &mut self,
        contract_address: &str,
        function_call: &FunctionCall,
        network: Option<&str>,
    ) -> Result<CallResult> {
        // Validate inputs
        let address = utils::validate_address(contract_address)
            .map_err(|e| anyhow!("Invalid contract address for simulation: {}", e))?;

        utils::validate_function_name(&function_call.function_name)
            .map_err(|e| anyhow!("Invalid function name: {}", e))?;

        if let Some(net) = network {
            let available_networks = self.provider_manager.get_available_networks();
            utils::validate_network(net, &available_networks)
                .map_err(|e| anyhow!("Network validation failed: {}", e))?;
        }

        let provider = self
            .provider_manager
            .get_provider(network)
            .map_err(|e| anyhow!("Failed to get provider: {}", e))?;

        // Get the ABI and encode the function call
        let abi = match self.abi_resolver.get_abi(contract_address, network).await {
            Ok(abi) => abi,
            Err(e) => {
                return Ok(CallResult {
                    success: false,
                    result: None,
                    error: Some(utils::interpret_abi_error(&e.to_string(), contract_address)),
                    gas_used: None,
                    transaction_hash: None,
                });
            }
        };

        let function = abi
            .functions()
            .find(|f| f.name == function_call.function_name)
            .ok_or_else(|| {
                let available_functions: Vec<String> = abi
                    .functions()
                    .map(|f| f.name.clone())
                    .collect();
                anyhow!("Function '{}' not found in contract ABI for simulation. Available functions: {}",
                    function_call.function_name, available_functions.join(", "))
            })?;

        let calldata = match self.encode_function_call(function, &function_call.parameters) {
            Ok(data) => data,
            Err(e) => {
                return Ok(CallResult {
                    success: false,
                    result: None,
                    error: Some(format!("Failed to encode function call: {}", e)),
                    gas_used: None,
                    transaction_hash: None,
                });
            }
        };

        // Build transaction request for simulation
        let mut tx_request = TransactionRequest::default()
            .to(address)
            .input(calldata.into());

        // Set from address if provided
        if let Some(from_str) = &function_call.from {
            match utils::validate_address(from_str) {
                Ok(from_address) => {
                    tx_request = tx_request.from(from_address);
                }
                Err(e) => {
                    return Ok(CallResult {
                        success: false,
                        result: None,
                        error: Some(format!("Invalid 'from' address for simulation: {}", e)),
                        gas_used: None,
                        transaction_hash: None,
                    });
                }
            }
        }

        // Set value if provided
        if let Some(value_str) = &function_call.value {
            match utils::validate_hex_value(value_str) {
                Ok(value) => {
                    tx_request = tx_request.value(value);
                }
                Err(e) => {
                    return Ok(CallResult {
                        success: false,
                        result: None,
                        error: Some(format!("Invalid transaction value for simulation: {}", e)),
                        gas_used: None,
                        transaction_hash: None,
                    });
                }
            }
        }

        // First, estimate gas for the transaction
        let gas_estimate = match provider.estimate_gas(&tx_request).await {
            Ok(gas) => Some(gas),
            Err(e) => {
                // If gas estimation fails, the transaction would likely fail
                let friendly_error = utils::interpret_rpc_error(&e.to_string());
                return Ok(CallResult {
                    success: false,
                    result: Some(serde_json::json!({
                        "simulated": true,
                        "gas_estimation_failed": true,
                        "error": friendly_error
                    })),
                    error: Some(format!(
                        "Gas estimation failed (transaction would likely revert): {}",
                        friendly_error
                    )),
                    gas_used: None,
                    transaction_hash: None,
                });
            }
        };

        // Simulate with eth_call
        match provider.call(&tx_request).await {
            Ok(result_bytes) => {
                // Try to decode the result
                let decoded_result = self
                    .decode_function_result(function, &result_bytes)
                    .unwrap_or_else(|_| {
                        serde_json::json!({
                            "raw_result": format!("0x{}", hex::encode(&result_bytes))
                        })
                    });

                Ok(CallResult {
                    success: true,
                    result: Some(serde_json::json!({
                        "simulated": true,
                        "result": decoded_result,
                        "would_succeed": true
                    })),
                    error: None,
                    gas_used: gas_estimate,
                    transaction_hash: None,
                })
            }
            Err(e) => {
                let friendly_error = utils::interpret_rpc_error(&e.to_string());
                Ok(CallResult {
                    success: false,
                    result: Some(serde_json::json!({
                        "simulated": true,
                        "would_succeed": false,
                        "revert_reason": friendly_error
                    })),
                    error: Some(format!("Transaction simulation failed: {}", friendly_error)),
                    gas_used: gas_estimate,
                    transaction_hash: None,
                })
            }
        }
    }

    /// Send a transaction to execute a contract function
    pub async fn send_transaction(
        &mut self,
        contract_address: &str,
        function_call: &FunctionCall,
        private_key: &str,
        gas_limit: Option<u64>,
        gas_price: Option<&str>,
        network: Option<&str>,
    ) -> Result<super::TransactionInfo> {
        use alloy::{
            network::{EthereumWallet, TransactionBuilder, ReceiptResponse},
            signers::local::PrivateKeySigner,
            providers::ProviderBuilder,
        };

        // Validate inputs
        let address = utils::validate_address(contract_address)
            .map_err(|e| anyhow!("Invalid contract address: {}", e))?;

        utils::validate_function_name(&function_call.function_name)
            .map_err(|e| anyhow!("Invalid function name: {}", e))?;

        if let Some(net) = network {
            let available_networks = self.provider_manager.get_available_networks();
            utils::validate_network(net, &available_networks)
                .map_err(|e| anyhow!("Network validation failed: {}", e))?;
        }

        // Parse and validate private key
        let private_key = private_key.trim();
        let private_key = if private_key.starts_with("0x") {
            &private_key[2..]
        } else {
            private_key
        };

        let signer = PrivateKeySigner::from_str(private_key)
            .map_err(|e| anyhow!("Invalid private key: {}", e))?;

        let from_address = signer.address();
        tracing::info!("Sending transaction from address: {:?}", from_address);

        // Get the ABI and encode the function call
        let abi = self
            .abi_resolver
            .get_abi(contract_address, network)
            .await
            .map_err(|e| {
                anyhow!(
                    "Could not resolve ABI for transaction: {}",
                    utils::interpret_abi_error(&e.to_string(), contract_address)
                )
            })?;

        let function = abi
            .functions()
            .find(|f| f.name == function_call.function_name)
            .ok_or_else(|| {
                let available_functions: Vec<String> = abi
                    .functions()
                    .map(|f| f.name.clone())
                    .collect();

                if available_functions.is_empty() {
                    anyhow!(
                        "Function '{}' not found. The contract ABI contains no functions.",
                        function_call.function_name
                    )
                } else {
                    anyhow!(
                        "Function '{}' not found in contract ABI. Available functions: {}",
                        function_call.function_name,
                        available_functions.join(", ")
                    )
                }
            })?;

        // Encode function call parameters
        let encoded_input = self
            .encode_function_call(function, &function_call.parameters)
            .map_err(|e| anyhow!("Failed to encode function call for transaction: {}", e))?;

        // Get provider and create wallet-enabled provider
        let base_provider = self.provider_manager.get_provider(network)?;
        let network_config = self.provider_manager.get_network_config(network)?;
        
        // Parse the URL for the wallet provider
        let url = network_config.rpc_url.parse()
            .map_err(|e| anyhow!("Invalid RPC URL '{}': {}", network_config.rpc_url, e))?;

        // Create wallet and provider for signing
        let wallet = EthereumWallet::from(signer);
        let provider = ProviderBuilder::new()
            .with_recommended_fillers()
            .wallet(wallet)
            .on_http(url);

        // Build the transaction request
        let mut tx_request = provider
            .transaction_request()
            .to(address)
            .input(encoded_input.into());

        // Set value if provided
        if let Some(value_str) = &function_call.value {
            let value = utils::validate_hex_value(value_str)
                .map_err(|e| anyhow!("Invalid transaction value: {}", e))?;
            tx_request = tx_request.value(value);
        }

        // Set gas limit
        if let Some(gas) = gas_limit {
            tx_request = tx_request.with_gas_limit(gas);
        } else {
            // Estimate gas if not provided
            match base_provider.estimate_gas(&tx_request.clone().from(from_address)).await {
                Ok(estimated_gas) => {
                    tx_request = tx_request.with_gas_limit(estimated_gas);
                }
                Err(e) => {
                    tracing::warn!("Gas estimation failed, using default: {}", e);
                    tx_request = tx_request.with_gas_limit(network_config.gas.default_gas_limit);
                }
            }
        }

        // Set gas price
        if let Some(gas_price_str) = gas_price {
            let gas_price = utils::validate_hex_value(gas_price_str)
                .map_err(|e| anyhow!("Invalid gas price: {}", e))?;
            tx_request = tx_request.with_gas_price(gas_price.to::<u128>());
        } else {
            // Use network's max gas price or get current gas price
            if let Some(max_gas_price) = network_config.gas.max_gas_price {
                tx_request = tx_request.with_gas_price(max_gas_price as u128);
            }
        }

        tracing::info!("Sending transaction to contract: {:?}", address);

        // Send the transaction
        match provider.send_transaction(tx_request).await {
            Ok(pending_tx) => {
                let tx_hash = *pending_tx.tx_hash();
                tracing::info!("Transaction sent with hash: {:?}", tx_hash);

                // Wait for confirmation
                match pending_tx.get_receipt().await {
                    Ok(receipt) => {
                        let success = receipt.status();
                        let gas_used = receipt.gas_used();
                        
                        Ok(super::TransactionInfo {
                            hash: format!("0x{:x}", tx_hash),
                            from: format!("0x{:x}", from_address),
                            to: Some(format!("0x{:x}", address)),
                            value: function_call.value.clone().unwrap_or_else(|| "0".to_string()),
                            gas_used: gas_used as u64,
                            gas_price: receipt.effective_gas_price.to_string(),
                            block_number: receipt.block_number.unwrap_or_default(),
                            timestamp: 0, // Would need to fetch block info for timestamp
                            status: success,
                        })
                    }
                    Err(e) => {
                        Err(anyhow!(
                            "Transaction was sent but confirmation failed: {}. Transaction hash: 0x{:x}",
                            e,
                            tx_hash
                        ))
                    }
                }
            }
            Err(e) => {
                Err(anyhow!(
                    "Failed to send transaction: {}",
                    utils::interpret_rpc_error(&e.to_string())
                ))
            }
        }
    }
}
