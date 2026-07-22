# c8y-oms-parser Microservice

A high-performance Rust-based microservice designed for Cumulocity. This service receives raw wireless M-Bus (wM-Bus) and OMS (Open Metering System) hex payloads from upstream microservices or external webhooks, decodes the DIF/VIF records, and returns structured data fields.

---

## Architecture & Integration Flow

+--------------------------+       HTTP POST /parse         +-------------------------+
|  Cumulocity MicroService | -----------------------------> |  c8y-oms-parser (Rust)  |
|                          | <----------------------------- |  (Port 80)              |
+--------------------------+    Decoded JSON Payload        +-------------------------+

1. The ** Microservice** sends a raw hexadecimal telemetry frame to the Rust microservice via Cumulocity's internal proxy (`http://cumulocity:8111/service/c8y-oms-parser/decode`).
2. The **Rust Microservice** parses the data frame and extracts individual measurement registers along with `HeaderRaw` (DIF+VIF+VIFE bytes), `RecordIndex`, values, and units.
3. The response is returned as a lightweight JSON object to be processed into Cumulocity `MeasurementRepresentation` objects.

---

## Features

- **DIF/VIF Parsing:** Decodes standard M-Bus data structures (Volume, Flow, Temperatures, Time, Battery Status).
- **Exact Field Tracking:** Exposes `HeaderRaw` and `RecordIndex` for every measurement, allowing client applications to distinguish between identical quantities (e.g., Forward Flow vs. Backward Flow volume).
- **Low Footprint:** Built with Rust for minimal memory consumption and rapid execution under heavy loads.

---

## Usage & API Reference

### Health Check
Cumulocity uses this endpoint for liveness and readiness probes.

* **URL:** \`GET /health\`
* **Response:** \`200 OK\`

```json
{
  "status": "UP"
}
```

---

### Payload Decoding Endpoint

* **URL:** `POST /parse`
* **Headers:** `Content-Type: application/json`

#### Example Request:
```json
{
  "payload": "2E44933B00000000046D33130701041300000000"
}
```

#### Example Response:
```json
{
  "ProgState": "Success",
  "DLL": {
    "DeviceType": "WaterMeter",
    "IdentificationNo": "09724574",
    "Manufacturer": "AXI"
  },
  "ParsedMeasurements": [
    {
      "RecordIndex": 0,
      "HeaderRaw": "046D",
      "DIF": "0x04",
      "VIF": "0x6D",
      "Name": "Date and Time",
      "Quantity": "Time",
      "Unit": "ISO8601",
      "Value": "2026-07-01T13:33:00"
    },
    {
      "RecordIndex": 1,
      "HeaderRaw": "0413",
      "DIF": "0x04",
      "VIF": "0x13",
      "Name": "Volume",
      "Quantity": "Volume",
      "Unit": "m³",
      "Value": 0.0
    },
    {
      "RecordIndex": 2,
      "HeaderRaw": "04933B",
      "DIF": "0x04",
      "VIF": "0x93",
      "Name": "Volume Accumulation (Forward Flow)",
      "Quantity": "Volume",
      "Unit": "m³",
      "Value": 12.45
    }
  ]
}
```

---

## HeaderRaw Reference Map

When mapping outputs in your downstream Microservice match against `HeaderRaw`:

| HeaderRaw | Description |
|---|---|
| `046D` | Date and Time string (ISO8601) |
| `0413` | Standard Volume (m³) |
| `04933B` | Forward Flow Volume Accumulation (m³) |
| `04933C` | Backward Flow Volume Accumulation (m³) |
| `023B` | Volume Flow Rate (m³/h) |
| `0259` | Flow Temperature (°C) |
| `01FD74` | Remaining Battery Life (Days) |

---

## Extending for Other OMS Payloads

Your application uses a **Data-Driven Meta Quantity Layout** backed by `VIF_LOOKUP_TABLE` and the `parse_vif()` function in `main.rs`.

### Step 1: Adding Standard VIF Rules (`VIF_LOOKUP_TABLE`)
To add support for a new unit or metric family (e.g., Reactive Power, Voltage, Current, or Gas), add a new `VifRule` struct to `VIF_LOOKUP_TABLE`:

```rust
const VIF_LOOKUP_TABLE: &[VifRule] = &[
    // ... existing rules ...

    // Example: Adding Voltage support (VIF range 0x48 - 0x4F)
    VifRule { 
        vif_mask: 0xF8, 
        vif_match: 0x48, 
        name: "Voltage", 
        unit: "V", 
        exponent_mapping: [-9, -8, -7, -6, -5, -4, -3, -2], 
        data_type: DataType::UnsignedInteger, 
        quantity: Quantity::Unknown 
    },
];
```

### Step 2: Adding VIFE / Extension Exceptions (`parse_vif`)
For multi-byte VIF extended codes (such as `0xFD` status bytes or specific sub-VIF definitions like forward/backward flow), update `parse_vif()`:

```rust
fn parse_vif(vif: u8, extended_vif_type: bool, current_vif: u8) -> MeasurementDescriptor {
    if extended_vif_type {
        // Example: Extended status table check
        if vif == 0xFD && current_vif == 0x17 {
            return MeasurementDescriptor { 
                name: "Error flags (binary)", 
                unit: "Bitmask", 
                exponent: 0, 
                data_type: DataType::UnsignedInteger, 
                quantity: Quantity::StatusAndDiagnostics 
            };
        }
        return MeasurementDescriptor { name: "Manufacturer Extension", unit: "None", exponent: 0, data_type: DataType::Unknown, quantity: Quantity::Unknown };
    }

    let vif_clean = vif & 0x7F;

    // Example: Handling specific sub-VIF byte combinations
    if vif_clean == 0x13 {
        if current_vif == 0x3B {
            return MeasurementDescriptor { name: "Volume Accumulation (Forward Flow)", unit: "m³", exponent: -3, data_type: DataType::UnsignedInteger, quantity: Quantity::Volume };
        }
    }

    // Falls back to VIF_LOOKUP_TABLE iteration...
}
```

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
