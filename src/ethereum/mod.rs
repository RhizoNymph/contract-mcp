pub mod abi;
pub mod contract;
pub mod provider;
pub mod utils;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractInfo {
    pub address: String,
    pub name: Option<String>,
    pub abi: serde_json::Value,
    pub bytecode: Option<String>,
    pub deployment_block: Option<u64>,
    pub creator: Option<String>,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionInfo {
    pub hash: String,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    pub gas_used: u64,
    pub gas_price: String,
    pub block_number: u64,
    pub timestamp: u64,
    pub status: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventInfo {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub block_number: u64,
    pub transaction_hash: String,
    pub log_index: u64,
    pub decoded: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub function_name: String,
    pub parameters: serde_json::Value,
    pub from: Option<String>,
    pub gas_limit: Option<u64>,
    pub gas_price: Option<String>,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallResult {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub gas_used: Option<u64>,
    pub transaction_hash: Option<String>,
}
