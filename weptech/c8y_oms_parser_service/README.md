# c8y-oms-parser Microservice

A high-performance, containerized Rust microservice built on **Axum** and **Tokio** designed for Cumulocity. It parses, decrypts (Mode 5 / Mode 7), and extracts telemetric records from Wireless M-Bus (wM-Bus) and Open Metering System (OMS) payloads.

---
## Features
* **Multi-Mode Decryption Support**: Engineered to parse unencrypted payloads as well as encrypted wM-Bus/OMS data streams utilizing Mode 5 (AES-CBC) and Mode 7 (AES-CTR/GCM) with dynamic IV resolution.
* **Driver Registry Architecture**: Extensible, trait-based driver dispatch system (`MeterDriver`) supporting manufacturer-specific profiles (Diehl, Axioma) alongside standard OMS fallbacks.
* **DIF/VIF Record Parsing**: Decodes standard and extended M-Bus data structures including Volume, Energy, Flow Rates, Temperatures, Time, Diagnostic Alarm Vectors, and Battery Status.
* **Exact Field & Record Tracking**: Exposes `header_raw`, `dib`, `vib`, `storage_no`, `tariff`, and `device` indices for every measurement, enabling upstream applications to differentiate identical physical quantities (e.g., Forward vs. Return Flow).
* **Enterprise Auditability & Key Masking**: Integrated `tracing` structured logging that automatically redacts sensitive AES keys and cleans payload attributes for secure, non-ANSI Cloud Logging Compliance.
* **Low Footprint & Async Throughput**: Built in pure Rust on top of Axum/Tokio for near-zero memory footprint and ultra-fast, concurrent execution under high-volume IoT microservice loads.
---

## Architecture & Integration Flow

```
  +-------------------------------------------------------------+
  |                   Cumulocity Platform.                      |
  +-------------------------------------------------------------+
                                |
                   HTTP POST /api/v1/parse
                   (Base64 Payload + Key + OMS Mode)
                                |
                                v
  +-------------------------------------------------------------+
  |                    Axum Web Framework                       |
  |  - Request Auditing & Sensitive Key Masking (`main.rs`)     |
  +-------------------------------------------------------------+
                                |
                                v
  +-------------------------------------------------------------+
  |                     DriverRegistry                          |
  |  - Header Extraction (M-Field, Device Type, Version)        |
  |  - Driver Dispatch (Exact Match -> Standard Fallback)       |
  +-------------------------------------------------------------+
         /                      |                      \
        v                       v                       v
+---------------+       +---------------+       +---------------+
| Axioma Driver |       | Diehl Driver  |       | Standard OMS  |
|  (AXI / ASI)  |       |   (HYDRUS)    |       | Driver        |
+---------------+       +---------------+       +---------------+
```
### Integration & Processing Flow
1. **Payload Ingestion**: Cumulocity forwards incoming LNS/Gateway payloads (`POST /api/v1/parse`) containing base64 frames and optional AES encryption keys.
2. **Middleware & Masking**: Axum handles incoming requests, logs execution metrics, and masks sensitive `encryptionkey` values before writing audit records.
3. **Header Inspection**: `DriverRegistry` extracts M-Bus Data Link Layer (DLL) header parameters (Manufacturer M-Field, Device Type, and Version).
4. **Driver Selection**: Registered drivers execute in priority sequence. Specific hardware drivers (e.g., `DiehlDriver`, `AxiomaDriver`) take precedence over generic standard handlers.
5. **Decryption & Deframe**: The selected driver handles Mode 5 (AES-CBC) or Mode 7 (AES-CTR/GCM) payload decryption using dynamic vector initialization.
6. **Measurement Extraction**: Records are converted into structured measurement objects containing values, units, tariffs, and storage numbers, then returned as JSON to Cumulocity.

### Core Characteristics
* **Runtime**: Axum on Tokio (Async I/O).
* **Target Container**: Docker (`linux/amd64`), binding on port `80`.
* **Logging & Auditing**: Non-ANSI, JSON-friendly structured logs (`tracing`) with automatic AES key redaction.
* **Extensibility**: Trait-based `MeterDriver` pattern enabling fast support for new manufacturer profiles without breaking core OMS parsing.
---

## Supported Drivers & Meters

| Driver Name | Manufacturer Code (M-Field) | Target Hardware | Decryption Support |
| :--- | :--- | :--- | :--- |
| **`DiehlDriver`** | `DME` (`0x11A5`) | Diehl HYDRUS Ultrasonic Water Meter | Mode 7 (AES-CTR/GCM) |
| **`AxiomaDriver`** | `AXI` / `ASI` | Axioma Qalcosonic W1 | Unencrypted, Mode 5 |
| **`StandardOmsDriver`**| Any Valid OMS Code | Generic OMS-compliant meters (Fallback) | Standard Mode 5 / Mode 7 |

---
## HeaderRaw Reference Map
When mapping outputs in downstream Cumulocity microservices, any other external service, match against the `header_raw` field in extracted measurements:
| `header_raw` | Description | Typical Unit | JSON Value Example |
| :--- | :--- | :--- | :--- |
| `046D` | Date and Time string | ISO8601 | `2026-08-04T10:00:00Z` |
| `0413` | Standard Volume | `m³` | `1245.892` |
| `04933B` | Forward Flow Volume Accumulation | `m³` | `1100.500` |
| `04933C` | Backward Flow Volume Accumulation | `m³` | `145.392` |
| `023B` | Volume Flow Rate | `m³/h` | `1.250` |
| `0259` | Flow Temperature | `°C` | `18.5` |
| `01FD74` | Remaining Battery Life | `months` | `97` |
---

## Supported DIF / DIFE / VIF / VIFE Reference
The core Rust parser (`dif_vif` module) decodes raw M-Bus frames using dynamic lookup tables. The primary standard VIF mappings and extended vendor overrides are detailed below:
    
### Standard VIF Lookup Table
| # | Metric Name | Unit | VIF Hex Range | Rust Decoder Type |
| :-: | :--- | :-: | :--- | :--- |
| **1** | Energy | `Wh` | `0x00 - 0x07` | Unsigned Integer / Scalar |
| **2** | Energy | `J` | `0x08 - 0x0F` | Unsigned Integer / Scalar |
| **3** | Energy | `Cal` | `0x60 - 0x67` | Unsigned Integer / Scalar |
| **4** | Volume | `m³` | `0x10 - 0x17` | Unsigned Integer / Scalar |
| **5** | Mass | `kg` | `0x18 - 0x1F` | Unsigned Integer / Scalar |
| **6** | On Time | `seconds` | `0x20 - 0x23` | Unsigned Integer |
| **7** | Operating Time | `seconds` | `0x24 - 0x27` | Unsigned Integer |
| **8** | Power | `W` | `0x28 - 0x2F` | Unsigned Integer / Scalar |
| **9** | Power | `J/h` | `0x30 - 0x37` | Unsigned Integer / Scalar |
| **10** | Volume Flow | `m³/h` | `0x38 - 0x3F` | Unsigned Integer / Scalar |
| **11** | Flow Temperature | `°C` | `0x58 - 0x5B` | Signed Integer / Scalar |
| **12** | Return Temperature | `°C` | `0x5C - 0x5F` | Signed Integer / Scalar |
| **13** | Temperature Difference | `K` | `0x60 - 0x63` | Signed Integer / Scalar |
| **14** | External Temperature | `°C` | `0x64 - 0x67` | Signed Integer / Scalar |
| **15** | Pressure | `bar` | `0x68 - 0x6B` | Signed Integer / Scalar |
| **16** | Date and Time | `ISO8601` | `0x6D` | MbusDateTime |


### Special VIF / Ext-VIF Overrides (`parse_vif`)
When extension bytes (`0xFD` / `0xFB`) or specific VIFE combination bytes are encountered, `parse_vif` redirects the parsing logic to specialized handlers:
| Trigger Bytes | Extracted Metric Name | Unit | Description / Note |
| :--- | :--- | :-: | :--- |
| `0xFD` + `0x74` | Remaining Battery Life | `days` | Converted from raw days counter |
| `0xFD` + `0x17` | Diagnostic Error Flags | `bitfield` | Device hardware error/alarm status |
| `0x13` + `0x3B` | Forward Volume Accumulation | `m³` | Sub-type direction tracking |
| `0x13` + `0x3C` | Backward Volume Accumulation | `m³` | Sub-type direction tracking |
    
> **Note**: Unrecognized VIF/DIF combinations are safely captured into raw fallback records with their `dib` and `vib` hex strings intact. This ensures payload parsing never fails the entire request batch due to unknown fields.
---



## REST API Specification

### Endpoint: `POST /api/v1/parse`

#### Request Payload
```json
{
  "payload": "iAEEA2k...",
  "oms_mode": 7,
  "encryptionkey": "00112233445566778899AABBCCDDEEFF"
}
```

* `payload` *(string, required)*: Base64-encoded raw wM-Bus radio frame.
* `oms_mode` *(number)*: Security mode override (`5` or `7`).
* `encryptionkey` *(string, optional)*: Hex-encoded AES key.

#### Successful Response (`200 OK`)
```json
{
  "status": "success",
  "data": {
    "driver_name": "DiehlDriver",
    "manufacturer": "DME",
    "device_type": 7,
    "dll": {
      "manufacturer_code": "DME",
      "device_type_raw": 7,
      "version": 99
    },
    "parsedMeasurements": [
      {
        "header_raw": "0413",
        "storage_no": 0,
        "tariff": 0,
        "device": 0,
        "dib": "04",
        "vib": "13",
        "value": "1245.892",
        "unit": "m³",
        "description": "Volume"
      }
    ],
    "payload_fields": {
      "mode": 7
    }
  },
  "error": null
}
```

#### Error Responses
* **`400 Bad Request`**: Base64 decoding error or invalid hex encryption key.
* **`422 Unprocessable Entity`**: Payload cannot be parsed or no compatible driver claimed the header.


---


## Adding Support for New Meters

The framework divides meters into two categories: **Strict OMS Standard** and **OMS Extensions / Vendor-Specific Profiles**.

```text
                      Is the meter 100% compliant 
                          with OMS DIF/VIF spec?
                             /              \
                            /                \
                         [YES]              [NO]
                          /                  \
                         v                    v
            Register Header in          Implement Custom
           `StandardOmsDriver`          `MeterDriver` Trait
```

---

### Scenario A: Adding a Meter that Strictly Follows the OMS Standard

If a new meter (e.g., standard water meter with manufacturer code `LGW`) uses default OMS DIF/VIF structures, you do **not** need to create a new driver file. Simply extend the candidate matching rules in `StandardOmsDriver`.

#### Step 1: Add New File (`src/drivers/standard_oms.rs`)

Create `src/drivers/standard_oms.rs` and implement the standard candidate matching and parsing routines:

```rust
use crate::traits::{MeterDriver, ProcessResult, DriverError, MBusHeader};
use crate::parser::dif_vif;

pub struct StandardOmsDriver;

impl StandardOmsDriver {
    pub fn new() -> Self {
        Self
    }
}

impl MeterDriver for StandardOmsDriver {
    fn name(&self) -> &'static str {
        "StandardOmsDriver"
    }

    fn supports(&self, header: &MBusHeader) -> bool {
        match header.manufacturer.as_str() {
            "LGW" | "GEN" | "OMS" => true,
            _ => false,
        }
    }

    fn parse(&self, payload: &[u8], oms_mode: Option<u8>, key: Option<&[u8]>) -> Result<ProcessResult, DriverError> {
        // Uses standard DIF/VIF chain parser
        let measurements = dif_vif::parse_standard_oms_records(payload, oms_mode, key)?;
        Ok(ProcessResult {
            driver_name: self.name().into(),
            manufacturer: "OMS".into(),
            device_type: 0x07,
            measurements,
            payload_fields: serde_json::json!({}),
        })
    }
}
```

#### Step 2: Register Candidate in DriverRegistry (`src/registry.rs`)

Ensure `StandardOmsDriver` is present in the pipeline execution list:

```rust
pub fn build_driver_registry() -> DriverRegistry {
    let mut registry = DriverRegistry::new();
    registry.register(Box::new(DiehlDriver::new()));
    registry.register(Box::new(AxiomaDriver::new()));
    
    // Catch-all standard fallback driver
    registry.register(Box::new(StandardOmsDriver::new())); 
    registry
}
```

---

### Scenario B: Adding a Meter that Extends or Deviates from OMS

When a manufacturer uses **proprietary VIF codes (`0xFD` / `0xFF`)**, non-standard header offsets, or custom encrypted payload layouts, implement a dedicated driver.

#### Step 1: Create the Driver Module (`src/drivers/custom_vendor.rs`)

Create a new file `src/drivers/custom_vendor.rs` implementing the `MeterDriver` trait:

```rust
use crate::traits::{MeterDriver, ProcessResult, DriverError, MBusHeader};
use crate::parser::dif_vif;

pub struct CustomVendorDriver;

impl CustomVendorDriver {
    pub fn new() -> Self {
        Self
    }
}

impl MeterDriver for CustomVendorDriver {
    fn name(&self) -> &'static str {
        "CustomVendorDriver"
    }

    fn supports(&self, header: &MBusHeader) -> bool {
        // Example: Manufacturer code "NB", Type 0x9E, Version 0xEE
        header.manufacturer == "NB" && header.device_type == 0x9E
    }

    fn parse(
        &self, 
        payload: &[u8], 
        oms_mode: Option<u8>, 
        key: Option<&[u8]>
    ) -> Result<ProcessResult, DriverError> {
        // 1. Perform custom transport header stripping or key/IV adjustments
        let decrypted = decrypt_vendor_payload(payload, oms_mode, key)?;

        // 2. Extract standard records using core DIF/VIF parser
        let mut measurements = dif_vif::parse_records(&decrypted)?;

        // 3. Handle Vendor-Specific Ext VIFs (e.g., 0xFD 0x17 for custom diagnostic)
        for measurement in measurements.iter_mut() {
            if measurement.vib == "FD17" {
                measurement.description = "Vendor Specific Alarm Vector".into();
                measurement.unit = "bitfield".into();
            }
        }

        Ok(ProcessResult {
            driver_name: self.name().into(),
            manufacturer: "NB".into(),
            device_type: 0x9E,
            measurements,
            payload_fields: serde_json::json!({ "custom_field": true }),
        })
    }
}
```

#### Step 2: Expose Module (`src/drivers/mod.rs`)

Export the new module:

```rust
pub mod axioma;
pub mod diehl;
pub mod standard_oms;
pub mod custom_vendor; // Add new module
```

#### Step 3: Register in Driver Registry (`src/registry.rs`)

Add the driver to the registry pipeline. **Order matters**: place specific vendor drivers *before* the generic fallback driver.

```rust
use crate::drivers::custom_vendor::CustomVendorDriver;

pub fn build_driver_registry() -> DriverRegistry {
    let mut registry = DriverRegistry::new();
    
    // Specific vendor drivers
    registry.register(Box::new(DiehlDriver::new()));
    registry.register(Box::new(AxiomaDriver::new()));
    registry.register(Box::new(CustomVendorDriver::new())); // Registered
    
    // Generic fallback driver
    registry.register(Box::new(StandardOmsDriver::new())); 
    
    registry
}
```

#### Step 4: Add Unit & Integration Tests (`tests/custom_vendor_test.rs`)

Verify driver detection and parsing with sample base64 payloads:

```rust
#[test]
fn test_custom_vendor_parsing() {
    let registry = build_driver_registry();
    let raw_payload = base64::decode("...").unwrap();
    
    let result = registry.process(&raw_payload, Some(7), None);
    assert!(result.is_ok());
    
    let process_data = result.unwrap();
    assert_eq!(process_data.driver_name, "CustomVendorDriver");
}
```

---

## Building and Packaging

Cumulocity microservices require a **Linux x86_64 Docker container** packaged inside a `.zip` archive alongside a `cumulocity.json` manifest.

### Prerequisites
- [Rust & Cargo](https://www.rust-lang.org/)
- [Docker](https://www.docker.com/)
- `zip` utility

---

### Step 1: Create the \`cumulocity.json\` Manifest
Ensure a \`cumulocity.json\` manifest file exists in your project root:

```json 
{
  "apiVersion": "2",
  "version": "1.0.3",
  "provider": {
    "name": "Cumulocity"
  },
  "isolation": "PER_TENANT",
  "replicas": 1,
  "contextPath": "c8y-oms-parser",
  "resources": {
    "memory": "512Mi",
    "cpu": "0.5"
  },
  "livenessProbe": {
    "httpGet": {
      "path": "/health",
      "port": 80
    },
    "initialDelaySeconds": 30,
    "periodSeconds": 10,
    "failureThreshold": 3
  },
  "readinessProbe": {
    "httpGet": {
      "path": "/health",
      "port": 80
    },
    "initialDelaySeconds": 20,
    "periodSeconds": 10,
    "failureThreshold": 3
  },
  "requiredRoles": [
    "ROLE_INVENTORY_READ"
  ],
  "roles": []
}
```

---

### Step 2: Build Local Docker Image
Build the container using multi-stage builds for small binary footprints:

```bash
docker build --platform linux/amd64 -t c8y-oms-parser:latest .
```

---

### Step 3: Package for Cumulocity

Cumulocity expects the Docker image saved as `image.tar` zipped together with `cumulocity.json`:

```bash
# 1. Export the Docker image as a tarball
docker save c8y-oms-parser:latest -o image.tar

# 2. Compress image.tar and cumulocity.json into a deployable zip file
zip c8y-oms-parser.zip image.tar cumulocity.json

# 3. Clean up the intermediary tar archive
rm image.tar
```

---

## Deployment to Cumulocity

1. Log into your **Cumulocity Tenant**.
2. Go to **Administration** -> **Ecosystem** -> **Microservices**.
3. Click **Add Microservice** and upload \`c8y-oms-parser.zip\`.
4. Verify that the health status turns **Green / UP**.
'''
