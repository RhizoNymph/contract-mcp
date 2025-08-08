use anyhow::Result;
use rmcp::{
    model::{ServerCapabilities, ServerInfo},
    tool,
    transport::stdio,
    ServerHandler, ServiceExt,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tracing::{error, info};

use crate::{
    config::Config,
    ethereum::{contract::ContractManager, provider::ProviderManager, FunctionCall},
};

#[derive(Debug, Clone)]
pub struct ContractMcpServer {
    contract_manager: Arc<tokio::sync::Mutex<ContractManager>>,
    #[allow(dead_code)]
    config: Arc<Config>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
struct ContractInfoRequest {
    address: String,
    network: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
struct ViewFunctionRequest {
    contract_address: String,
    function_name: String,
    parameters: Value,
    network: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
struct EstimateGasRequest {
    contract_address: String,
    function_name: String,
    parameters: Value,
    from: Option<String>,
    value: Option<String>,
    network: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
struct ContractEventsRequest {
    contract_address: String,
    from_block: Option<u64>,
    to_block: Option<u64>,
    network: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
struct SimulateTransactionRequest {
    contract_address: String,
    function_name: String,
    parameters: Value,
    from: Option<String>,
    value: Option<String>,
    network: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
struct SendTransactionRequest {
    contract_address: String,
    function_name: String,
    parameters: Value,
    private_key: String,
    value: Option<String>,
    gas_limit: Option<u64>,
    gas_price: Option<String>,
    network: Option<String>,
}

impl ContractMcpServer {
    pub fn new(config: Config) -> Result<Self> {
        let provider_manager = ProviderManager::new(config.clone())?;
        let contract_manager = Arc::new(tokio::sync::Mutex::new(ContractManager::new(
            provider_manager,
        )));
        let config = Arc::new(config);

        Ok(Self {
            contract_manager,
            config,
        })
    }

    pub async fn run(&self) -> Result<()> {
        info!("Starting Contract MCP Server");

        let service = self.clone().serve(stdio()).await?;

        info!("Contract MCP Server started successfully");
        let _ = service.waiting().await;
        Ok(())
    }
}

#[tool(tool_box)]
impl ContractMcpServer {
    #[tool(description = "Get information about an Ethereum smart contract")]
    async fn get_contract_info(&self, #[tool(aggr)] request: ContractInfoRequest) -> String {
        let mut manager = self.contract_manager.lock().await;

        match manager
            .get_contract_info(&request.address, request.network.as_deref())
            .await
        {
            Ok(info) => serde_json::to_string_pretty(&info)
                .unwrap_or_else(|_| "Failed to serialize contract info".to_string()),
            Err(e) => {
                error!("Failed to get contract info: {}", e);
                format!("Error: {}", e)
            }
        }
    }

    #[tool(description = "Call a read-only contract function")]
    async fn call_view_function(&self, #[tool(aggr)] request: ViewFunctionRequest) -> String {
        let mut manager = self.contract_manager.lock().await;

        let function_call = FunctionCall {
            function_name: request.function_name,
            parameters: request.parameters,
            from: None,
            gas_limit: None,
            gas_price: None,
            value: None,
        };

        match manager
            .call_view_function(
                &request.contract_address,
                &function_call,
                request.network.as_deref(),
            )
            .await
        {
            Ok(result) => serde_json::to_string_pretty(&result)
                .unwrap_or_else(|_| "Failed to serialize result".to_string()),
            Err(e) => {
                error!("Failed to call view function: {}", e);
                format!("Error: {}", e)
            }
        }
    }

    #[tool(description = "Estimate gas cost for a contract function call")]
    async fn estimate_gas(&self, #[tool(aggr)] request: EstimateGasRequest) -> String {
        let mut manager = self.contract_manager.lock().await;

        let function_call = FunctionCall {
            function_name: request.function_name,
            parameters: request.parameters,
            from: request.from,
            gas_limit: None,
            gas_price: None,
            value: request.value,
        };

        match manager
            .estimate_gas(
                &request.contract_address,
                &function_call,
                request.network.as_deref(),
            )
            .await
        {
            Ok(gas_estimate) => format!("Estimated gas: {} units", gas_estimate),
            Err(e) => {
                error!("Failed to estimate gas: {}", e);
                format!("Error: {}", e)
            }
        }
    }

    #[tool(description = "Get events emitted by a smart contract")]
    async fn get_contract_events(&self, #[tool(aggr)] request: ContractEventsRequest) -> String {
        let manager = self.contract_manager.lock().await;

        match manager
            .get_contract_events(
                &request.contract_address,
                request.from_block,
                request.to_block,
                request.network.as_deref(),
            )
            .await
        {
            Ok(events) => serde_json::to_string_pretty(&events)
                .unwrap_or_else(|_| "Failed to serialize events".to_string()),
            Err(e) => {
                error!("Failed to get contract events: {}", e);
                format!("Error: {}", e)
            }
        }
    }

    #[tool(description = "Simulate a contract transaction without executing it")]
    async fn simulate_transaction(
        &self,
        #[tool(aggr)] request: SimulateTransactionRequest,
    ) -> String {
        let mut manager = self.contract_manager.lock().await;

        let function_call = FunctionCall {
            function_name: request.function_name,
            parameters: request.parameters,
            from: request.from,
            gas_limit: None,
            gas_price: None,
            value: request.value,
        };

        match manager
            .simulate_transaction(
                &request.contract_address,
                &function_call,
                request.network.as_deref(),
            )
            .await
        {
            Ok(result) => serde_json::to_string_pretty(&result)
                .unwrap_or_else(|_| "Failed to serialize result".to_string()),
            Err(e) => {
                error!("Failed to simulate transaction: {}", e);
                format!("Error: {}", e)
            }
        }
    }

    #[tool(description = "Send a transaction to execute a contract function")]
    async fn send_transaction(&self, #[tool(aggr)] request: SendTransactionRequest) -> String {
        // Check if write operations are allowed
        if !self.config.security.allow_write_operations {
            return format!("Error: Write operations are disabled. Use --allow-writes flag to enable transaction sending.");
        }

        let function_call = FunctionCall {
            function_name: request.function_name,
            parameters: request.parameters,
            from: None, // Will be derived from private key
            gas_limit: request.gas_limit,
            gas_price: request.gas_price.clone(),
            value: request.value,
        };

        let mut manager = self.contract_manager.lock().await;

        match manager
            .send_transaction(
                &request.contract_address,
                &function_call,
                &request.private_key,
                request.gas_limit,
                request.gas_price.as_deref(),
                request.network.as_deref(),
            )
            .await
        {
            Ok(result) => serde_json::to_string_pretty(&result)
                .unwrap_or_else(|_| "Failed to serialize result".to_string()),
            Err(e) => {
                error!("Failed to send transaction: {}", e);
                format!("Error: {}", e)
            }
        }
    }
}

#[tool(tool_box)]
impl ServerHandler for ContractMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("MCP server for interacting with Ethereum smart contracts using Alloy. Supports contract inspection, function calls, gas estimation, event retrieval, transaction simulation, and contract transaction sending.".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
