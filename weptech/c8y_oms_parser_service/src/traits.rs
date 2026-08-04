//Defines core data structures, error types, and the driver interface (MeterDriver).

// src/traits.rs

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Header too short (minimum 10 bytes required)")]
    HeaderTooShort,

    #[error("Header error: {0}")]
    HeaderError(String),

    #[error("No matching driver found for M-Field: {manufacturer}, Device Type: {device_type:#04x}, Version: {version:#04x}")]
    NoMatchingDriver {
        manufacturer: String,
        device_type: u8,
        version: u8,
    },

    #[error("Decryption failed: {0}")]
    DecryptionError(String),

    #[error("Invalid frame structure: {0}")]
    InvalidFrame(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeterIdentifier {
    pub manufacturer: String,
    pub device_type: Option<u8>,
    pub version: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedMeasurements {
    pub header_raw: String,
    pub storage_no: u32,
    pub tariff: u32,
    pub device: u32,
    pub dib: String,
    pub vib: String,
    pub value: String,
    pub unit: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DllHeaderInfo {
    pub manufacturer_code: String,
    pub device_type_raw: u8,
    pub version: u8,
}

#[derive(Debug, Clone)]
pub struct DllHeader {
    pub length: u8,
    pub c_field: u8,
    pub m_field: [u8; 2],
    pub a_field: [u8; 6],
    pub version: u8,
    pub device_type: u8,
}

impl DllHeader {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ParseError> {
        if bytes.len() < 10 {
            return Err(ParseError::HeaderTooShort);
        }

        let mut m_field = [0u8; 2];
        m_field.copy_from_slice(&bytes[2..4]);

        let mut a_field = [0u8; 6];
        a_field.copy_from_slice(&bytes[4..10]);

        Ok(Self {
            length: bytes[0],
            c_field: bytes[1],
            m_field,
            a_field,
            version: bytes[8],
            device_type: bytes[9],
        })
    }

    pub fn manufacturer_code(&self) -> String {
        let m = u16::from_le_bytes(self.m_field);
        let c1 = (((m >> 10) & 0x1F) as u8 + 64) as char;
        let c2 = (((m >> 5) & 0x1F) as u8 + 64) as char;
        let c3 = ((m & 0x1F) as u8 + 64) as char;
        format!("{}{}{}", c1, c2, c3)
    }

    pub fn manufacturer_hex(&self) -> String {
        format!("{:02X}{:02X}", self.m_field[1], self.m_field[0])
    }
}

// ============================================================================
// MBUS DECODING HELPERS
// ============================================================================

fn parse_unsigned(bytes: &[u8]) -> u64 {
    let mut val = 0u64;
    for (i, &b) in bytes.iter().take(8).enumerate() {
        val |= (b as u64) << (i * 8);
    }
    val
}

fn parse_signed(bytes: &[u8]) -> i64 {
    if bytes.is_empty() {
        return 0;
    }
    let len = bytes.len().min(8);
    let raw = parse_unsigned(&bytes[..len]);
    let bits = len * 8;
    if bits == 64 {
        raw as i64
    } else {
        let shift = 64 - bits;
        ((raw << shift) as i64) >> shift
    }
}

fn parse_bcd(bytes: &[u8]) -> Option<u64> {
    let mut val = 0u64;
    let mut multiplier = 1u64;

    for &b in bytes {
        let low = b & 0x0F;
        let high = (b >> 4) & 0x0F;

        if low > 9 || high > 9 {
            return None;
        }

        val += (low as u64) * multiplier;
        multiplier *= 10;
        val += (high as u64) * multiplier;
        multiplier *= 10;
    }

    Some(val)
}

fn parse_mbus_datetime_f(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 4 {
        return None;
    }
    let min = (bytes[0] & 0x3F) as u32;
    let hour = (bytes[1] & 0x1F) as u32;
    let day = (bytes[2] & 0x1F) as u32;

    let year_low = (bytes[2] >> 5) & 0x07;
    let year_high = (bytes[3] >> 4) & 0x0F;
    let year_offset = ((year_high << 3) | year_low) as i32;
    let year = 2000 + year_offset;

    let month = (bytes[3] & 0x0F) as u32;

    if (1..=12).contains(&month) && (1..=31).contains(&day) && hour < 24 && min < 60 {
        return Some(format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:00",
            year, month, day, hour, min
        ));
    }

    None
}

fn evaluate_record(
    primary_dif: u8,
    primary_vif: u8,
    vif_stack: &[u8],
    data: &[u8],
) -> (String, String, String) {
    let data_type = primary_dif & 0x0F;
    let vif_clean = primary_vif & 0x7F;

    // 1. Special Data (DIF 0x0D)
    if data_type == 0x0D {
        return ("Special Data".into(), "hex".into(), hex::encode(data).to_uppercase());
    }

    // 2. Date and Time Type F (VIF 0x6D)
    if vif_clean == 0x6D {
        if let Some(formatted_dt) = parse_mbus_datetime_f(data) {
            return ("Date and Time".into(), "".into(), formatted_dt);
        }
    }

    // 3. Extension Tables (0xFD / 0x7D)
    if primary_vif == 0xFD || primary_vif == 0x7D {
        let vife = vif_stack.get(1).copied().unwrap_or(0) & 0x7F;
        let raw_val = parse_unsigned(data);

        match vife {
            // 0x17 in FD table is Error Flags / Status
            0x17 => {
                return (
                    "Error Flags".into(),
                    "none".into(),
                    raw_val.to_string(),
                );
            }
            0x20 | 0x21 => {
                return (
                    "Remaining Battery Lifetime".into(),
                    "days".into(),
                    raw_val.to_string(),
                );
            }
            // 0x24, 0x34, 0x74 are Battery Lifetime in Months
            0x24 | 0x34 | 0x74 => {
                return (
                    "Remaining Battery Lifetime".into(),
                    "months".into(),
                    raw_val.to_string(),
                );
            }
            0x25 => {
                return (
                    "Remaining Battery Lifetime".into(),
                    "years".into(),
                    raw_val.to_string(),
                );
            }
            _ => {
                return (
                    "Extended Metric".into(),
                    "raw".into(),
                    raw_val.to_string(),
                );
            }
        }
    }

    // 4. Operating / On-Time Duration (VIF 0x20 - 0x27)
    if (0x20..=0x27).contains(&vif_clean) {
        let raw_val = parse_unsigned(data);
        let (name, unit) = match vif_clean {
            0x20 => ("Operating Time", "seconds"),
            0x21 => ("Operating Time", "minutes"),
            0x22 => ("Operating Time", "hours"),
            0x23 => ("Operating Time", "days"),
            0x24 => ("On Time Duration", "seconds"),
            0x25 => ("On Time Duration", "minutes"),
            0x26 => ("On Time Duration", "hours"),
            0x27 => ("On Time Duration", "days"),
            _ => ("Operating Time", "units"),
        };
        return (name.into(), unit.into(), raw_val.to_string());
    }

    // 5. Signed Temperature Evaluation (VIF 0x58..=0x67)
    if (0x58..=0x67).contains(&vif_clean) {
        let exp = (vif_clean & 0x03) as i32 - 3;
        let scale = 10f64.powi(exp);

        let signed_val = if data_type == 0x02 && data.len() == 2 {
            let bytes: [u8; 2] = data.try_into().unwrap_or([0; 2]);
            i16::from_le_bytes(bytes) as f64
        } else {
            parse_signed(data) as f64
        };

        let val = (signed_val * scale * 100.0).round() / 100.0;
        let name = match vif_clean {
            0x58..=0x5B => "Flow Temperature",
            0x5C..=0x5F => "Return Temperature",
            0x60..=0x63 => "Temperature Difference",
            _ => "External Temperature",
        };
        return (name.into(), "°C".into(), format!("{:.2}", val));
    }

    // 6. Flow Evaluation (0x30..=0x3F)
    if (0x30..=0x3F).contains(&vif_clean) {
        let raw_val = parse_unsigned(data);
        let exp = (vif_clean & 0x07) as i32 - 6;
        let scale = 10f64.powi(exp);
        let val = (raw_val as f64) * scale;
        return ("Current Flow".into(), "m³/h".into(), format!("{:.3}", val));
    }

    // 7. Volume Evaluation (0x10..=0x17)
    if (0x10..=0x17).contains(&vif_clean) {
        let is_bcd = matches!(data_type, 0x09 | 0x0A | 0x0B | 0x0C | 0x0E);
        let raw_val = if is_bcd {
            parse_bcd(data).unwrap_or_else(|| parse_unsigned(data))
        } else {
            parse_unsigned(data)
        };

        let exp = (vif_clean & 0x07) as i32 - 6;
        let scale = 10f64.powi(exp);
        let val = (raw_val as f64) * scale;
        return ("Volume".into(), "m³".into(), format!("{:.2}", val));
    }

    let fallback_val = parse_unsigned(data);
    ("Unknown Metric".into(), "raw".into(), format!("{:.2}", fallback_val as f64))
}

// ============================================================================
// PUBLIC MBUS STREAM PARSER
// ============================================================================

pub fn parse_wmbus_records(raw_bytes: &[u8]) -> Vec<ParsedMeasurements> {
    let mut processed_records = Vec::new();
    if raw_bytes.is_empty() {
        return processed_records;
    }

    let mut idx = 0;

    // Fast-forward initial header or filler bytes
    if raw_bytes.len() >= 2 && raw_bytes[0] == 0x2F && raw_bytes[1] == 0x2F {
        while idx < raw_bytes.len() && raw_bytes[idx] == 0x2F {
            idx += 1;
        }
    } else if raw_bytes.len() >= 4 {
        match raw_bytes[0] {
            0x78 | 0x7A => idx = 5,
            0x72 | 0x73 | 0x7E => idx = 13,
            _ => {
                if raw_bytes.len() >= 12 && (raw_bytes[10] == 0x78 || raw_bytes[10] == 0x7A) {
                    idx = 15;
                } else if raw_bytes.len() >= 12 && (raw_bytes[10] == 0x72 || raw_bytes[10] == 0x7E) {
                    idx = 23;
                } else {
                    idx = 0;
                }
            }
        }
    }

    while idx < raw_bytes.len() {
        let start_idx = idx;
        let dif = raw_bytes[idx];

        if dif == 0x2F {
            idx += 1;
            continue;
        }

        if dif == 0x0F || dif == 0x1F {
            break;
        }

        // 1. Traverse DIF & DIFE stack
        let mut dif_stack = Vec::new();
        loop {
            if idx >= raw_bytes.len() { break; }
            let byte = raw_bytes[idx];
            idx += 1;
            dif_stack.push(byte);
            if (byte & 0x80) == 0 { break; }
        }

        if idx >= raw_bytes.len() { break; }

        // 2. Traverse VIF & VIFE stack
        let mut vif_stack = Vec::new();
        loop {
            if idx >= raw_bytes.len() { break; }
            let byte = raw_bytes[idx];
            idx += 1;
            vif_stack.push(byte);
            if (byte & 0x80) == 0 { break; }
        }

        let header_end_idx = idx;
        let header_bytes = &raw_bytes[start_idx..header_end_idx];
        let header_raw_hex = hex::encode(header_bytes).to_uppercase();

        let primary_dif = dif_stack[0];
        let primary_vif = vif_stack[0];

        let data_type = primary_dif & 0x0F;

        let data_len = match data_type {
            0x00 => 0,
            0x01 | 0x09 => 1,
            0x02 | 0x0A => 2,
            0x03 | 0x0B => 3,
            0x04 | 0x0C | 0x05 => 4,
            0x06 | 0x0E => 6,
            0x07 | 0x0F => 8,
            0x0D => {
                if idx < raw_bytes.len() {
                    let len = raw_bytes[idx] as usize;
                    idx += 1;
                    len
                } else { 0 }
            }
            _ => 0,
        };

        if idx + data_len > raw_bytes.len() {
            break;
        }

        let data_segment = &raw_bytes[idx..idx + data_len];
        idx += data_len;

        let (name, unit, value) = evaluate_record(
            primary_dif,
            primary_vif,
            &vif_stack,
            data_segment,
        );

        processed_records.push(ParsedMeasurements {
            header_raw: header_raw_hex,
            storage_no: 0,
            tariff: 0,
            device: 0,
            dib: format!("{:02X}", primary_dif),
            vib: format!("{:02X}", primary_vif),
            value,
            unit,
            description: name,
        });
    }

    processed_records
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessResult {
    pub driver_name: String,
    pub manufacturer: String,
    pub device_type: u8,
    pub dll: DllHeaderInfo,
    pub parsedMeasurements: Vec<ParsedMeasurements>,
    pub payload_fields: serde_json::Value,
}

pub trait MeterDriver: Send + Sync {
    fn supports(&self, header: &DllHeader) -> bool;
    fn parse(
        &self,
        raw_payload: &[u8],
        oms_mode: Option<u8>,
        key: Option<&[u8]>,
    ) -> Result<ProcessResult, ParseError>;
}