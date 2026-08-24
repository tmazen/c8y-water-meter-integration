

use crate::traits::{DllHeader, DllHeaderInfo, MeterDriver, ParseError, ProcessResult};
use serde_json::json;

pub struct GenericOmsFallbackDriver;

impl MeterDriver for GenericOmsFallbackDriver {
    fn supports(&self, _header: &DllHeader) -> bool {
        true
    }

    fn parse(
        &self,
        raw_payload: &[u8],
        _oms_mode: Option<u8>,
        _key: Option<&[u8]>,
    ) -> Result<ProcessResult, ParseError> {
        let header = DllHeader::from_bytes(raw_payload)?;

        Ok(ProcessResult {
            driver_name: "Generic OMS Fallback".into(),
            manufacturer: "GEN".into(),
            device_type: 0x00,
            dll: DllHeaderInfo {
                manufacturer_code: header.manufacturer_hex(),
                device_type_raw: header.device_type,
                version: header.version,
            },
            parsed_measurements: vec![],
            payload_fields: json!({
                "raw_parsed": true
            }),
        })
    }
}