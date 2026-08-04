// src/drivers/diehl.rs

use serde_json::json;
use crate::security::{EncryptionMode, SecurityEngine};
use crate::traits::{
    parse_wmbus_records, DllHeader, DllHeaderInfo, MeterDriver, ParseError, ProcessResult,
};

pub struct DiehlDriver;

impl DiehlDriver {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DiehlDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl MeterDriver for DiehlDriver {
    fn supports(&self, header: &DllHeader) -> bool {
        header.manufacturer_code() == "DME"
    }

    fn parse(
        &self,
        raw_payload: &[u8],
        oms_mode: Option<u8>,
        key: Option<&[u8]>,
    ) -> Result<ProcessResult, ParseError> {
        let header = DllHeader::from_bytes(raw_payload)?;

        // Fallback to Mode 7 if no mode is explicitly passed
        let mode = EncryptionMode::from(oms_mode.or(Some(7)));

        let decrypted_payload =
            SecurityEngine::decrypt_and_sanitize(mode, raw_payload, 10, key)?;

        let parsedMeasurements = parse_wmbus_records(&decrypted_payload);

        let dll_info = DllHeaderInfo {
            manufacturer_code: header.manufacturer_code(),
            device_type_raw: header.device_type,
            version: header.version,
        };

        Ok(ProcessResult {
            driver_name: "DiehlDriver".to_string(),
            manufacturer: header.manufacturer_code(),
            device_type: header.device_type,
            dll: dll_info,
            parsedMeasurements,
            payload_fields: json!({
                "mode": oms_mode.unwrap_or(7),
            }),
        })
    }
}