//Defines HTTP request and response models matching your exact requested JSON schema

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct ParseRequest {
    pub payload: String,
    pub oms_mode: Option<u8>,
    pub encryptionkey: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct DllInfo {
    pub device_type: String,
    pub identification_no: String,
    pub manufacturer: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ParsedMeasurementResponse {
    pub record_index: usize,
    pub header_raw: String,
    pub dif: String,
    pub vif: String,
    pub name: String,
    pub quantity: String,
    pub unit: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ParseResponse {
    pub prog_state: String,
    #[serde(rename = "DLL")]
    pub dll: DllInfo,
    pub parsed_measurements: Vec<ParsedMeasurementResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ErrorResponse {
    pub prog_state: String,
    pub error: String,
}