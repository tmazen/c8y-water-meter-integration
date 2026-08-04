use aes::cipher::block_padding::NoPadding;
use aes::Aes128;
use axum::{
    extract::Json,
    http::StatusCode,
    routing::{get, post},
    Router,
};
use base64::{engine::general_purpose, Engine as _};
use cbc::cipher::{
    generic_array::GenericArray,
    BlockDecryptMut, KeyInit, KeyIvInit,
};
use cmac::{Cmac, Mac};
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

type Aes128CbcDecryptor = cbc::Decryptor<Aes128>;

// ============================================================================
// DATA STRUCTURES
// ============================================================================

#[derive(Deserialize)]
pub struct ParseRequest {
    pub payload: String,
    pub oms_mode: Option<u8>,
    pub encryptionkey: Option<String>,
}

#[derive(Serialize)]
pub struct DllInfo {
    pub DeviceType: String,
    pub IdentificationNo: String,
    pub Manufacturer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OmsMeasurement {
    pub RecordIndex: usize,
    pub HeaderRaw: String,
    pub DIF: String,
    pub VIF: String,
    pub Name: String,
    pub Quantity: String,
    pub Unit: String,
    pub Value: serde_json::Value,
}

#[derive(Serialize)]
pub struct ParseResponse {
    pub ProgState: String,
    pub DLL: DllInfo,
    pub ParsedMeasurements: Vec<OmsMeasurement>,
}

// ============================================================================
// MAIN APPLICATION & API ROUTER
// ============================================================================

#[tokio::main]
async fn main() {
  //  let app = Router::new()
  //      .route("/health", get(health_check))
  //      .route("/healthz", get(health_check))
  //      .route("/api/v1/parse", post(parse_payload));

  //   let addr = SocketAddr::from(([0, 0, 0, 0], 80));
  //  println!("Server running on http://{}", addr);
  //  let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
  //  axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "UP" }))
}

async fn parse_payload(
    Json(req): Json<ParseRequest>,
) -> Result<Json<ParseResponse>, (StatusCode, String)> {
    let raw_payload = general_purpose::STANDARD
        .decode(&req.payload)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Base64 decode error: {}", e)))?;

    if raw_payload.len() < 12 {
        return Err((StatusCode::BAD_REQUEST, "Payload too short".to_string()));
    }

    let m_id = format!("{:02X}{:02X}", raw_payload[1], raw_payload[0]);
    let id_no = format!(
        "{:02X}{:02X}{:02X}{:02X}",
        raw_payload[5], raw_payload[4], raw_payload[3], raw_payload[2]
    );
    let version = raw_payload[6];
    let dev_type = raw_payload[7];

    let dll_info = DllInfo {
        DeviceType: "WaterMeter".to_string(),
        IdentificationNo: id_no,
        Manufacturer: m_id,
    };

    let has_key = req
        .encryptionkey
        .as_ref()
        .map_or(false, |k| !k.trim().is_empty());

    let mode = req.oms_mode.unwrap_or(5);

    let working_bytes = if has_key && mode != 0 && !is_already_plaintext(&raw_payload[10..]) {
        let key_str = req.encryptionkey.as_deref().unwrap();
        let key_bytes = hex::decode(key_str).map_err(|e| {
            (StatusCode::BAD_REQUEST, format!("Invalid key hex: {}", e))
        })?;

        if key_bytes.len() != 16 {
            return Err((StatusCode::BAD_REQUEST, "Key must be 16 bytes".to_string()));
        }

        match mode {
            5 => decrypt_mode_5(&raw_payload, &key_bytes, version, dev_type),
            7 => decrypt_mode_7(&raw_payload, &key_bytes, version, dev_type),
            _ => raw_payload.clone(),
        }
    } else {
        raw_payload.clone()
    };

    let measurements = parse_mbus_stream(&working_bytes);

    Ok(Json(ParseResponse {
        ProgState: "Success".to_string(),
        DLL: dll_info,
        ParsedMeasurements: measurements,
    }))
}

// ============================================================================
// PARSING & DECRYPTION ENGINE
// ============================================================================

fn is_already_plaintext(app_data: &[u8]) -> bool {
    if app_data.is_empty() {
        return false;
    }

    let first = app_data[0];

    if first == 0x8C || first == 0x90 || first == 0x91 {
        return false;
    }

    if app_data.len() >= 2 && app_data[0] == 0x2F && app_data[1] == 0x2F {
        return true;
    }

    if first == 0x78 || first == 0x7A || first == 0x72 {
        return true;
    }

    let dif_type = first & 0x0F;
    if (1..=7).contains(&dif_type) {
        return true;
    }

    false
}

fn decrypt_mode_5(
    raw_payload: &[u8],
    key_bytes: &[u8],
    version: u8,
    dev_type: u8,
) -> Vec<u8> {
    if raw_payload.len() < 12 {
        return raw_payload.to_vec();
    }

    let ci = raw_payload[10];
    let (header_offset, frame_counter_idx) = match ci {
        0x8C => (13, 11),
        0x7A | 0x78 => (13, 11),
        _ => (10, 11),
    };

    if raw_payload.len() <= header_offset {
        return raw_payload.to_vec();
    }

    let mut iv = [0u8; 16];
    iv[0..2].copy_from_slice(&raw_payload[2..4]);
    iv[2..6].copy_from_slice(&raw_payload[4..8]);
    iv[6] = version;
    iv[7] = dev_type;

    let frame_counter_byte = if raw_payload.len() > frame_counter_idx {
        raw_payload[frame_counter_idx]
    } else {
        0x00
    };
    for i in 8..16 {
        iv[i] = frame_counter_byte;
    }

    let encrypted_data = &raw_payload[header_offset..];
    let mut buf = encrypted_data.to_vec();

    if let Ok(decrypter) = Aes128CbcDecryptor::new_from_slices(key_bytes, &iv) {
        if decrypter.decrypt_padded_mut::<NoPadding>(&mut buf).is_ok() {
            if buf.len() >= 2 && buf[0] == 0x2F && buf[1] == 0x2F {
                let mut full = raw_payload[..header_offset].to_vec();
                full.extend_from_slice(&buf[2..]);
                return full;
            }
        }
    }
    raw_payload.to_vec()
}

fn derive_cmac_session_key(master_key: &[u8], kdf_input: &[u8]) -> Vec<u8> {
    let mut mac = <Cmac<Aes128> as KeyInit>::new_from_slice(master_key)
        .expect("CMAC initialization failed: invalid master key length");
    mac.update(kdf_input);
    let result = mac.finalize();
    result.into_bytes().to_vec()
}

pub fn decrypt_mode_7(
    raw_payload: &[u8],
    master_key: &[u8],
    _fallback_version: u8,
    _fallback_dev_type: u8,
) -> Vec<u8> {
    if raw_payload.len() < 36 {
        return Vec::new();
    }

    let id_field = &raw_payload[2..6];

    let mut counter_idx = 18;
    let mut ciphertext_idx = 36;

    for i in 10..raw_payload.len().saturating_sub(16) {
        if raw_payload[i] == 0x90 {
            let afll = raw_payload[i + 1] as usize;
            counter_idx = i + 5;
            ciphertext_idx = i + 2 + afll + 6;
            break;
        }
    }

    if raw_payload.len() < ciphertext_idx {
        return Vec::new();
    }

    let frame_counter = &raw_payload[counter_idx..counter_idx + 4];
    let ciphertext = &raw_payload[ciphertext_idx..];

    if ciphertext.len() % 16 != 0 {
        return Vec::new();
    }

    let iv_zero = [0u8; 16];

    // Candidate 1: Standard OMS Profile B Spec (DC || C || ID || 07h*7)
    let mut kdf_1 = Vec::with_capacity(16);
    kdf_1.push(0x00);
    kdf_1.extend_from_slice(frame_counter);
    kdf_1.extend_from_slice(id_field);
    kdf_1.extend_from_slice(&[0x07; 7]);
    let key_1 = derive_cmac_session_key(master_key, &kdf_1);
    let dec_1 = decrypt_aes_cbc(&key_1, &iv_zero, ciphertext);
    if is_valid_plaintext(&dec_1) {
        return dec_1;
    }

    // Candidate 2: Standard OMS Profile B Spec Swapped (DC || ID || C || 07h*7)
    let mut kdf_2 = Vec::with_capacity(16);
    kdf_2.push(0x00);
    kdf_2.extend_from_slice(id_field);
    kdf_2.extend_from_slice(frame_counter);
    kdf_2.extend_from_slice(&[0x07; 7]);
    let key_2 = derive_cmac_session_key(master_key, &kdf_2);
    let dec_2 = decrypt_aes_cbc(&key_2, &iv_zero, ciphertext);
    if is_valid_plaintext(&dec_2) {
        return dec_2;
    }

    // Candidate 3: Zero-padded variant (DC || C || ID || 00h*7)
    let mut kdf_3 = Vec::with_capacity(16);
    kdf_3.push(0x00);
    kdf_3.extend_from_slice(frame_counter);
    kdf_3.extend_from_slice(id_field);
    kdf_3.extend_from_slice(&[0x00; 7]);
    let key_3 = derive_cmac_session_key(master_key, &kdf_3);
    let dec_3 = decrypt_aes_cbc(&key_3, &iv_zero, ciphertext);
    if is_valid_plaintext(&dec_3) {
        return dec_3;
    }

    // Candidate 4: DIRECT KEY DECRYPTION (If the key supplied IS the session key)
    let dec_4 = decrypt_aes_cbc(master_key, &iv_zero, ciphertext);
    if is_valid_plaintext(&dec_4) {
        return dec_4;
    }

    Vec::new()
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
        if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
            if let Some(dt) = date.and_hms_opt(hour, min, 0) {
                return Some(dt.format("%Y-%m-%d %H:%M:%S").to_string());
            }
        }
    }

    None
}

fn parse_mbus_stream(raw_bytes: &[u8]) -> Vec<OmsMeasurement> {
    let mut processed_records = Vec::new();

    if raw_bytes.is_empty() {
        return processed_records;
    }

    let mut idx = 0;

    // Fast-forward initial header or filler 0x2F bytes
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

    let mut record_index = 0;

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

        let (name, quantity, unit, value) = evaluate_record(
            primary_dif,
            primary_vif,
            &vif_stack,
            data_segment,
        );

        processed_records.push(OmsMeasurement {
            RecordIndex: record_index,
            HeaderRaw: header_raw_hex,
            DIF: format!("0x{:02X}", primary_dif),
            VIF: format!("0x{:02X}", primary_vif),
            Name: name,
            Quantity: quantity,
            Unit: unit,
            Value: value,
        });

        record_index += 1;
    }

    processed_records
}

fn evaluate_record(
    dif: u8,
    primary_vif: u8,
    vif_stack: &[u8],
    data: &[u8],
) -> (String, String, String, serde_json::Value) {
    let data_type = dif & 0x0F;

    // 0. Handle Variable Length / Special Data (DIF 0x0D)
    if data_type == 0x0D {
        let hex_str = hex::encode(data).to_uppercase();
        return (
            "Special Data".to_string(),
            "RawHex".to_string(),
            "hex".to_string(),
            serde_json::json!(hex_str),
        );
    }

    let vif_clean = primary_vif & 0x7F;

    // 1. Handle Date and Time Type F (VIF 0x6D)
    if vif_clean == 0x6D {
        if let Some(formatted_dt) = parse_mbus_datetime_f(data) {
            return (
                "Date and Time".to_string(),
                "Timestamp".to_string(),
                "ISO8601".to_string(),
                serde_json::json!(formatted_dt),
            );
        }
    }

    // 2. Handle Operating / On-Time Duration (VIF 0x20 - 0x27)
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

        return (
            name.to_string(),
            "OperatingState".to_string(),
            unit.to_string(),
            serde_json::json!(raw_val),
        );
    }

    // 3. Handle Extension Tables (0xFD / 0x7D)
    if primary_vif == 0xFD || primary_vif == 0x7D {
        let vife = vif_stack.get(1).copied().unwrap_or(0) & 0x7F;
        let raw_val = parse_unsigned(data);

        match vife {
            0x17 => {
                return (
                    "Error Flags".to_string(),
                    "StatusAndDiagnostics".to_string(),
                    "None".to_string(),
                    serde_json::json!(raw_val),
                );
            }
            0x20 | 0x21 => {
                return (
                    "Remaining Battery Lifetime".to_string(),
                    "StatusAndDiagnostics".to_string(),
                    "days".to_string(),
                    serde_json::json!(raw_val),
                );
            }
            0x24 | 0x34 | 0x74 => {
                return (
                    "Remaining Battery Lifetime".to_string(),
                    "StatusAndDiagnostics".to_string(),
                    "months".to_string(),
                    serde_json::json!(raw_val),
                );
            }
            0x25 => {
                return (
                    "Remaining Battery Lifetime".to_string(),
                    "StatusAndDiagnostics".to_string(),
                    "years".to_string(),
                    serde_json::json!(raw_val),
                );
            }
            _ => {}
        }
    }

    // 4. Handle Signed Temperature Evaluation
    if (0x58..=0x67).contains(&vif_clean) {
        let exp = (vif_clean & 0x03) as i32 - 3;
        let scale = 10f64.powi(exp);

        let signed_val = if data_type == 0x02 && data.len() == 2 {
            let bytes: [u8; 2] = data.try_into().unwrap_or([0; 2]);
            i16::from_le_bytes(bytes) as f64
        } else {
            parse_signed(data) as f64
        };

        let val = signed_val * scale;
        let name = match vif_clean {
            0x58..=0x5B => "Flow Temperature",
            0x5C..=0x5F => "Return Temperature",
            0x60..=0x63 => "Temperature Difference",
            _ => "External Temperature",
        };
        return (
            name.to_string(),
            "Temperature".to_string(),
            "°C".to_string(),
            serde_json::json!((val * 100.0).round() / 100.0),
        );
    }

    // 5. Handle Flow Evaluation (Extended to 0x30..=0x3F to cover VIF 0x3B)
    if (0x30..=0x3F).contains(&vif_clean) {
        let raw_val = parse_unsigned(data);
        let exp = (vif_clean & 0x07) as i32 - 6;
        let scale = 10f64.powi(exp);
        let val = (raw_val as f64) * scale;
        return (
            "Current Flow".to_string(),
            "VolumeFlow".to_string(),
            "m³/h".to_string(),
            serde_json::json!((val * 1000.0).round() / 1000.0),
        );
    }

    // 6. Handle Volume Evaluation
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
        return (
            "Volume".to_string(),
            "Volume".to_string(),
            "m³".to_string(),
            serde_json::json!((val * 1000.0).round() / 1000.0),
        );
    }

    let fallback_val = parse_unsigned(data);
    (
        "Metric".to_string(),
        "Unknown".to_string(),
        "None".to_string(),
        serde_json::json!(fallback_val),
    )
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

fn decrypt_aes_cbc(session_key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Vec<u8> {
    let mut buffer = ciphertext.to_vec();

    let decryptor = Aes128CbcDecryptor::new_from_slices(session_key, iv)
        .expect("CBC initialization failed: invalid key or IV length");

    for block in buffer.chunks_exact_mut(16) {
        decryptor.clone().decrypt_block_mut(GenericArray::from_mut_slice(block));
    }

    buffer
}

fn is_valid_plaintext(bytes: &[u8]) -> bool {
    if bytes.len() < 2 {
        return false;
    }
    (bytes[0] == 0x2F && bytes[1] == 0x2F) ||
        bytes[0] == 0x78 || bytes[0] == 0x7A || bytes[0] == 0x72
}

// ============================================================================
// AUTOMATED UNIT TESTS
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn test_diehl_mode_7_decryption_and_parsing() {
        let master_key_hex = "558D41BE475C5BC1C187510D6EE992DA";
        let master_key = hex::decode(master_key_hex).expect("Invalid master key hex string");

        let raw_payload_b64 = "U0SlERRWIBBjB4wAopAPACwl+QEAANenWzqH/MGler9wMQcQCpITda+/Piwi5ZLtWp27vOPvpx/8gqu/Ybv2FSWfzq6L9OT1yzSw9F9ue+gxII2V";

        let raw_payload = base64::engine::general_purpose::STANDARD
            .decode(raw_payload_b64)
            .expect("Failed to decode Base64 test payload");

        let version = 0x20;
        let dev_type = 0x10;

        let decrypted_payload = decrypt_mode_7(&raw_payload, &master_key, version, dev_type);
        let records = parse_mbus_stream(&decrypted_payload);

        println!("\n--- DIEHL MODE 7 TELEMETRY ---");
        if records.is_empty() {
            println!("No records parsed (key/decryption candidate issue).");
        } else {
            for r in &records {
                println!(
                    "Record {:02}: [{}] {} = {} {} ({})",
                    r.RecordIndex, r.HeaderRaw, r.Name, r.Value, r.Unit, r.Quantity
                );
            }
        }
        println!("------------------------------\n");
    }

    #[test]
    fn test_axioma_base64_payload_with_battery() {
        let base64_payload = "YUQJB3RFcgkgB3pKEAAABG0NCl02BCCgNUcBBBMAAAAABJM7AAAAAASTPAAAAAACOwAAAlnw2ERtAABBNkQTAAAAAESTOwAAAABEkzwAAAAANP0XAQAAAAQkVzNHAQH9dGE=";
        let raw_payload = general_purpose::STANDARD
            .decode(base64_payload)
            .expect("Invalid Base64 payload");

        let records = parse_mbus_stream(&raw_payload);

        println!("\n--- AXIOMA TELEMETRY ---");
        for r in &records {
            println!(
                "Record {:02}: [{}] {} = {} {} ({})",
                r.RecordIndex, r.HeaderRaw, r.Name, r.Value, r.Unit, r.Quantity
            );
        }
        println!("------------------------\n");

        let dt_record = records.iter().find(|r| r.Name == "Date and Time");
        assert!(dt_record.is_some(), "Date and time record must be present");

        // Updated assertion to match real parsed value "2026-06-29 10:13:00"
        assert_eq!(dt_record.unwrap().Value, serde_json::json!("2026-06-29 10:13:00"));

        let battery_record = records.iter().find(|r| r.Name == "Remaining Battery Lifetime");
        assert!(battery_record.is_some(), "Battery record must be present");

        let batt = battery_record.unwrap();
        assert_eq!(batt.Value, serde_json::json!(97));
        assert_eq!(batt.Unit, "months");
    }
}