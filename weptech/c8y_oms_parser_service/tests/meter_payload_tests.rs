// tests/meter_payload_tests.rs

use base64::engine::general_purpose;
use base64::Engine;
use c8y_oms_parser_service::registry::build_driver_registry;

#[test]
fn test_diehl_mode_7_decryption_and_parsing() {
    let registry = build_driver_registry();

    let master_key_hex = "558D41BE475C5BC1C187510D6EE992DA";

    let master_key = hex::decode(master_key_hex).expect("Invalid master key hex string");

    let raw_payload_b64 = "U0SlERRWIBBjB4wAopAPACwl+QEAANenWzqH/MGler9wMQcQCpITda+/Piwi5ZLtWp27vOPvpx/8gqu/Ybv2FSWfzq6L9OT1yzSw9F9ue+gxII2V";

    let raw_payload = general_purpose::STANDARD
        .decode(raw_payload_b64)
        .expect("Failed to decode Base64 test payload");

    // Process through driver registry (Mode 7)
    let result = registry.process(&raw_payload, Some(7), Some(&master_key));

    assert!(result.is_ok(), "Driver failed to process payload: {:?}", result.err());
    let process_result = result.unwrap();

    println!("\n--- DIEHL MODE 7 TELEMETRY ---");
    for (idx, r) in process_result.parsed_measurements.iter().enumerate() {
        println!(
            "Record {:02}: [{}] {} = {} {} ({})",
            idx, r.header_raw, r.description, r.value, r.unit, r.dib
        );
    }
    println!("Payload JSON: {:#?}", process_result.payload_fields);
    println!("------------------------------\n");

    // Assert that measurements were successfully decrypted and parsed
    assert!(
        !process_result.parsed_measurements.is_empty(),
        "Expected parsed measurements, got empty list"
    );
}

#[test]
fn test_axioma_base64_payload_with_battery() {
    let registry = build_driver_registry();

    let base64_payload = "YUQJB3RFcgkgB3pKEAAABG0NCl02BCCgNUcBBBMAAAAABJM7AAAAAASTPAAAAAACOwAAAlnw2ERtAABBNkQTAAAAAESTOwAAAABEkzwAAAAANP0XAQAAAAQkVzNHAQH9dGE=";
    let raw_payload = general_purpose::STANDARD
        .decode(base64_payload)
        .expect("Invalid Base64 payload");

    // Unencrypted payload -> pass None for oms_mode and key
    let result = registry.process(&raw_payload, None, None);

    if let Err(ref e) = result {
        panic!("Axioma payload failed to process: {:?}", e);
    }
    let process_result = result.unwrap();

    let records = &process_result.parsed_measurements;

    println!("\n--- AXIOMA TELEMETRY ---");
    for (idx, r) in records.iter().enumerate() {
        println!(
            "Record {:02}: [{}] {} = {} {} ({})",
            idx, r.header_raw, r.description, r.value, r.unit, r.dib
        );
    }
    println!("------------------------\n");

    // 1. Verify Date and Time record
    let dt_record = records.iter().find(|r| r.description == "Date and Time");
    assert!(dt_record.is_some(), "Date and time record must be present");
    assert_eq!(dt_record.unwrap().value, "2026-06-29 10:13:00");

    // 2. Verify Remaining Battery Lifetime record
    let battery_record = records.iter().find(|r| r.description == "Remaining Battery Lifetime");
    assert!(battery_record.is_some(), "Battery record must be present");

    let batt = battery_record.unwrap();
    assert_eq!(batt.value, "97");
    assert_eq!(batt.unit, "days");
}