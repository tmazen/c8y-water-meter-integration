use axum::{
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use base64::{Engine as _, engine::general_purpose};
use m_bus_parser::mbus_data::MbusData;
use m_bus_parser::WirelessFrame;

#[derive(Deserialize)]
struct ParseRequest {
    payload: String,
}

// -------------------------------------------------------------------------
// DATA-DRIVEN META QUANTITY LAYOUT
// -------------------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq)]
enum DataType {
    SignedInteger,
    UnsignedInteger,
    RealFloat,
    Bcd,
    MbusDateTime,
    Variable,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Quantity {
    Energy,
    Volume,
    Mass,
    Power,
    Temperature,
    Pressure,
    Time,
    StatusAndDiagnostics,
    Unknown,
}

struct VifRule {
    vif_mask: u8,
    vif_match: u8,
    name: &'static str,
    unit: &'static str,
    exponent_mapping: [i8; 8],
    data_type: DataType,
    quantity: Quantity,
}

const VIF_LOOKUP_TABLE: &[VifRule] = &[
    VifRule { vif_mask: 0xF8, vif_match: 0x00, name: "Energy", unit: "Wh", exponent_mapping: [-3, -2, -1, 0, 1, 2, 3, 4], data_type: DataType::UnsignedInteger, quantity: Quantity::Energy },
    VifRule { vif_mask: 0xF8, vif_match: 0x08, name: "Energy", unit: "J", exponent_mapping: [-3, -2, -1, 0, 1, 2, 3, 4], data_type: DataType::UnsignedInteger, quantity: Quantity::Energy },
    VifRule { vif_mask: 0xF8, vif_match: 0x60, name: "Energy", unit: "Cal", exponent_mapping: [-3, -2, -1, 0, 1, 2, 3, 4], data_type: DataType::UnsignedInteger, quantity: Quantity::Energy },
    VifRule { vif_mask: 0xF8, vif_match: 0x10, name: "Volume", unit: "m³", exponent_mapping: [-6, -5, -4, -3, -2, -1, 0, 1], data_type: DataType::UnsignedInteger, quantity: Quantity::Volume },
    VifRule { vif_mask: 0xF8, vif_match: 0x18, name: "Mass", unit: "kg", exponent_mapping: [-3, -2, -1, 0, 1, 2, 3, 4], data_type: DataType::UnsignedInteger, quantity: Quantity::Mass },
    VifRule { vif_mask: 0xFC, vif_match: 0x20, name: "On Time", unit: "seconds", exponent_mapping: [0, 1, 2, 3, 0, 0, 0, 0], data_type: DataType::UnsignedInteger, quantity: Quantity::Time },
    VifRule { vif_mask: 0xFC, vif_match: 0x24, name: "Operating Time", unit: "seconds", exponent_mapping: [0, 1, 2, 3, 0, 0, 0, 0], data_type: DataType::UnsignedInteger, quantity: Quantity::Time },
    VifRule { vif_mask: 0xF8, vif_match: 0x28, name: "Power", unit: "W", exponent_mapping: [-3, -2, -1, 0, 1, 2, 3, 4], data_type: DataType::UnsignedInteger, quantity: Quantity::Power },
    VifRule { vif_mask: 0xF8, vif_match: 0x30, name: "Power", unit: "J/h", exponent_mapping: [-3, -2, -1, 0, 1, 2, 3, 4], data_type: DataType::UnsignedInteger, quantity: Quantity::Power },
    VifRule { vif_mask: 0xF8, vif_match: 0x38, name: "Volume Flow", unit: "m³/h", exponent_mapping: [-6, -5, -4, -3, -2, -1, 0, 1], data_type: DataType::UnsignedInteger, quantity: Quantity::Volume },
    VifRule { vif_mask: 0xFC, vif_match: 0x58, name: "Flow Temperature", unit: "°C", exponent_mapping: [-3, -2, -1, 0, 0, 0, 0, 0], data_type: DataType::SignedInteger, quantity: Quantity::Temperature },
    VifRule { vif_mask: 0xFC, vif_match: 0x5C, name: "Return Temperature", unit: "°C", exponent_mapping: [-3, -2, -1, 0, 0, 0, 0, 0], data_type: DataType::SignedInteger, quantity: Quantity::Temperature },
    VifRule { vif_mask: 0xFC, vif_match: 0x60, name: "Temperature Difference", unit: "K", exponent_mapping: [-3, -2, -1, 0, 0, 0, 0, 0], data_type: DataType::SignedInteger, quantity: Quantity::Temperature },
    VifRule { vif_mask: 0xFC, vif_match: 0x64, name: "External Temperature", unit: "°C", exponent_mapping: [-3, -2, -1, 0, 0, 0, 0, 0], data_type: DataType::SignedInteger, quantity: Quantity::Temperature },
    VifRule { vif_mask: 0xFC, vif_match: 0x68, name: "Pressure", unit: "bar", exponent_mapping: [-3, -2, -1, 0, 0, 0, 0, 0], data_type: DataType::SignedInteger, quantity: Quantity::Pressure },
    VifRule { vif_mask: 0xFF, vif_match: 0x6D, name: "Date and Time", unit: "ISO8601", exponent_mapping: [0, 0, 0, 0, 0, 0, 0, 0], data_type: DataType::MbusDateTime, quantity: Quantity::Time },
];

struct MeasurementDescriptor {
    name: &'static str,
    unit: &'static str,
    exponent: i8,
    data_type: DataType,
    quantity: Quantity,
}

fn parse_vif(vif: u8, extended_vif_type: bool, current_vif: u8) -> MeasurementDescriptor {
    if extended_vif_type {
        if vif == 0xFD && current_vif == 0x74 {
            return MeasurementDescriptor { name: "Remaining Battery Life", unit: "day(s)", exponent: 0, data_type: DataType::UnsignedInteger, quantity: Quantity::StatusAndDiagnostics };
        }
        if vif == 0xFD && current_vif == 0x17 {
            return MeasurementDescriptor { name: "Error flags (binary)", unit: "Bitmask", exponent: 0, data_type: DataType::UnsignedInteger, quantity: Quantity::StatusAndDiagnostics };
        }
        return MeasurementDescriptor { name: "Manufacturer Extension", unit: "None", exponent: 0, data_type: DataType::Unknown, quantity: Quantity::Unknown };
    }

    let vif_clean = vif & 0x7F;

    // --- ENHANCED: SPECIFIC MAPPING FOR VOLUME ACCUMULATION CODES ---
    if vif_clean == 0x13 {
        if current_vif == 0x3B {
            return MeasurementDescriptor { name: "Volume Accumulation (Forward Flow)", unit: "m³", exponent: -3, data_type: DataType::UnsignedInteger, quantity: Quantity::Volume };
        }
        if current_vif == 0x3C {
            return MeasurementDescriptor { name: "Volume Accumulation (Backward Flow)", unit: "m³", exponent: -3, data_type: DataType::UnsignedInteger, quantity: Quantity::Volume };
        }
    }
    // ---------------------------------------------------------------

    for rule in VIF_LOOKUP_TABLE {
        if (vif_clean & rule.vif_mask) == rule.vif_match {
            let index = (vif_clean & !rule.vif_mask) as usize;
            let exponent = rule.exponent_mapping.get(index).cloned().unwrap_or(0);

            return MeasurementDescriptor {
                name: rule.name,
                unit: rule.unit,
                exponent,
                data_type: rule.data_type,
                quantity: rule.quantity,
            };
        }
    }

    MeasurementDescriptor {
        name: "Unknown Metric",
        unit: "None",
        exponent: 0,
        data_type: DataType::Unknown,
        quantity: Quantity::Unknown,
    }
}

fn sign_extend(raw: u64, bytes_len: usize) -> i64 {
    if bytes_len == 0 || bytes_len > 8 { return raw as i64; }
    let bits = bytes_len * 8;
    let shift = 64 - bits;
    ((raw << shift) as i64) >> shift
}

fn parse_bcd(bytes: &[u8]) -> u64 {
    let mut val: u64 = 0;
    for &b in bytes.iter().rev() {
        let high = (b >> 4) & 0x0F;
        let low = b & 0x0F;
        val = val * 100 + (high as u64) * 10 + (low as u64);
    }
    val
}

// -------------------------------------------------------------------------
// RUNTIME CORE PIPELINE ENTRYPOINT
// -------------------------------------------------------------------------
#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/api/v1/parse", post(oms_parser_handler));

    println!("OMS Parser Service listening on port 80");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:80").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> (StatusCode, &'static str) {
    (StatusCode::OK, "{\"status\":\"UP\"}")
}

async fn oms_parser_handler(
    Json(body): Json<ParseRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let sanitized_payload: String = body.payload
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '+' || *c == '/' || *c == '=')
        .collect();

    let raw_bytes = match general_purpose::STANDARD.decode(&sanitized_payload) {
        Ok(bytes) => bytes,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("Base64 decoding failed: {}", e) })),
            );
        }
    };

    match MbusData::<WirelessFrame>::try_from(raw_bytes.as_slice()) {
        Ok(parsed_frame) => {
            let manufacturer_code = format!(
                "{}{}{}",
                parsed_frame.frame.manufacturer_id.manufacturer_code.code[0],
                parsed_frame.frame.manufacturer_id.manufacturer_code.code[1],
                parsed_frame.frame.manufacturer_id.manufacturer_code.code[2]
            );
            let identification_no = format!("{:08}", parsed_frame.frame.manufacturer_id.identification_number.number);
            let device_type = format!("{:?}", parsed_frame.frame.manufacturer_id.device_type);

            let mut processed_records = Vec::new();

            let app_data_offset = 10;
            let mut idx = app_data_offset;

            if idx < raw_bytes.len() {
                let ci_byte = raw_bytes[idx];
                let tpl_header_len = match ci_byte {
                    0x72 | 0x73 | 0x76 | 0x7D => 0,
                    0x7A => 4,
                    0x7E => 12,
                    _ => 0
                };
                idx += 1 + tpl_header_len;
            }

            let mut record_index = 0;

            while idx < raw_bytes.len() {
                let start_idx = idx; // Track exact position where record headers begin

                let dif = raw_bytes[idx];
                if dif == 0x2F || dif == 0x0F || dif == 0x1F || dif == 0x7F {
                    idx += 1;
                    continue;
                }

                idx += 1;

                // Read DIFE Chaining Loops
                let mut current_dif = dif;
                let mut storage_number: u32 = (dif as u32 & 0x40) >> 6;
                let mut tariff: u32 = 0;
                let mut dife_count = 0;

                while (current_dif & 0x80) != 0 {
                    if idx >= raw_bytes.len() {
                        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Unexpected end of frame while parsing DIFE chain" })));
                    }
                    current_dif = raw_bytes[idx];
                    idx += 1;

                    storage_number |= (current_dif as u32 & 0x0F) << (1 + dife_count * 4);
                    tariff |= ((current_dif as u32 & 0x30) >> 4) << (dife_count * 2);
                    dife_count += 1;
                }

                if idx >= raw_bytes.len() {
                    return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Unexpected end of frame after parsing DIFE" })));
                }

                // Read VIF & VIFE Chaining Loops
                let vif = raw_bytes[idx];
                idx += 1;
                let mut current_vif = vif;
                let mut extended_vif_type = vif == 0xFD || vif == 0xFB;

                while (current_vif & 0x80) != 0 {
                    if idx >= raw_bytes.len() {
                        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Unexpected end of frame while parsing VIFE chain" })));
                    }
                    current_vif = raw_bytes[idx];
                    idx += 1;
                }

                // Capture full raw header bytes right before reading data segment length
                let header_end_idx = idx;
                let header_bytes = &raw_bytes[start_idx..header_end_idx];
                let header_raw_hex = hex::encode(header_bytes).to_uppercase();

                let mut data_len = 0;
                let mut active_data_type = DataType::Unknown;

                let dif_nibble = dif & 0x0F;
                if dif_nibble == 0x0D {
                    if idx < raw_bytes.len() {
                        data_len = raw_bytes[idx] as usize;
                        idx += 1;
                        active_data_type = DataType::Variable;
                    }
                } else {
                    data_len = match dif_nibble {
                        0x00 => 0,
                        0x01 | 0x09 => 1,
                        0x02 | 0x0A => 2,
                        0x03 | 0x0B => 3,
                        0x04 | 0x0C => 4,
                        0x05 => 4,
                        0x06 | 0x0E => 6,
                        0x07 | 0x0F => 8,
                        _ => 0,
                    };

                    active_data_type = match dif_nibble {
                        0x09..=0x0F => DataType::Bcd,
                        0x05 => DataType::RealFloat,
                        _ => DataType::Unknown,
                    };
                }

                if idx + data_len > raw_bytes.len() {
                    return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Data segment length bounds violation" })));
                }
                let data_segment = &raw_bytes[idx..idx + data_len];
                idx += data_len;

                let descriptor = parse_vif(vif, extended_vif_type, current_vif);
                let descriptor_name = descriptor.name;

                let current_type = if active_data_type == DataType::Unknown {
                    descriptor.data_type
                } else {
                    active_data_type
                };

                let mut final_value = serde_json::Value::Null;

                match current_type {
                    DataType::UnsignedInteger => {
                        let mut raw_num: u64 = 0;
                        for (i, &byte) in data_segment.iter().enumerate() {
                            raw_num |= (byte as u64) << (i * 8);
                        }
                        let scaled_val = (raw_num as f64) * 10.0f64.powi(descriptor.exponent as i32);
                        final_value = serde_json::json!(scaled_val);
                    },
                    DataType::SignedInteger => {
                        let mut raw_num: u64 = 0;
                        for (i, &byte) in data_segment.iter().enumerate() {
                            raw_num |= (byte as u64) << (i * 8);
                        }
                        let signed_extended = sign_extend(raw_num, data_len);
                        let scaled_val = (signed_extended as f64) * 10.0f64.powi(descriptor.exponent as i32);
                        final_value = serde_json::json!(scaled_val);
                    },
                    DataType::RealFloat => {
                        if data_len == 4 {
                            let arr = [data_segment[0], data_segment[1], data_segment[2], data_segment[3]];
                            let raw_float = f32::from_le_bytes(arr) as f64;
                            let scaled_val = raw_float * 10.0f64.powi(descriptor.exponent as i32);
                            final_value = serde_json::json!(scaled_val);
                        }
                    },
                    DataType::Bcd => {
                        let raw_bcd = parse_bcd(data_segment);
                        let scaled_val = (raw_bcd as f64) * 10.0f64.powi(descriptor.exponent as i32);
                        final_value = serde_json::json!(scaled_val);
                    },
                    DataType::MbusDateTime => {
                        if data_len >= 4 {
                            let minute = data_segment[0] & 0x3F;
                            let hour = data_segment[1] & 0x1F;
                            let day = data_segment[2] & 0x1F;
                            let month = data_segment[3] & 0x0F;
                            let year = ((data_segment[2] & 0xE0) >> 5) | ((data_segment[3] & 0xF0) >> 1);
                            final_value = serde_json::json!(format!("20{:02}-{:02}-{:02}T{:02}:{:02}:00", year, month, day, hour, minute));
                        }
                    },
                    DataType::Variable => {
                        if data_segment.iter().all(|&b| b.is_ascii_graphic() || b.is_ascii_whitespace()) {
                            final_value = serde_json::json!(String::from_utf8_lossy(data_segment).to_string());
                        } else {
                            final_value = serde_json::json!(data_segment);
                        }
                    },
                    DataType::Unknown => {
                        final_value = serde_json::json!(data_segment);
                    }
                }

                processed_records.push(serde_json::json!({
                    "RecordIndex": record_index,
                    "HeaderRaw": header_raw_hex,
                    "Name": descriptor_name,
                    "Quantity": format!("{:?}", descriptor.quantity),
                    "Value": final_value,
                    "Unit": descriptor.unit,
                    "StorageNumber": storage_number,
                    "Tariff": tariff,
                    "DIF": format!("0x{:02X}", dif),
                    "VIF": format!("0x{:02X}", vif)
                }));

                record_index += 1;
            }

            let final_output = serde_json::json!({
                "ProgState": "Success",
                "DLL": {
                    "Manufacturer": manufacturer_code,
                    "IdentificationNo": identification_no,
                    "DeviceType": device_type
                },
                "ParsedMeasurements": processed_records
            });

            (StatusCode::OK, Json(final_output))
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": format!("M-Bus Parse Failure: {:?}", e) })),
        ),
    }
}