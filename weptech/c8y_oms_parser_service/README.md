# c8y-oms-parser Microservice

A high-performance Rust-based microservice designed for Cumulocity. This service receives raw wireless M-Bus (wM-Bus) and OMS (Open Metering System) hex payloads from upstream microservices or external webhooks, decodes the DIF/VIF records, and returns structured data fields.

---

## Architecture & Integration Flow

+--------------------------+       HTTP POST /decode        +-------------------------+
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

* **URL:** \`POST /decode\`
* **Headers:** \`Content-Type: application/json\`

#### Example Request:
\`\`\`json
{
  \"payload\": \"2E44933B00000000046D33130701041300000000\"
}
\`\`\`

#### Example Response:
\`\`\`json
{
  \"ProgState\": \"Success\",
  \"DLL\": {
    \"DeviceType\": \"WaterMeter\",
    \"IdentificationNo\": \"09724574\",
    \"Manufacturer\": \"AXI\"
  },
  \"DecodedMeasurements\": [
    {
      \"RecordIndex\": 0,
      \"HeaderRaw\": \"046D\",
      \"DIF\": \"0x04\",
      \"VIF\": \"0x6D\",
      \"Name\": \"Date and Time\",
      \"Quantity\": \"Time\",
      \"Unit\": \"ISO8601\",
      \"Value\": \"2026-07-01T13:33:00\"
    },
    {
      \"RecordIndex\": 1,
      \"HeaderRaw\": \"0413\",
      \"DIF\": \"0x04\",
      \"VIF\": \"0x13\",
      "Name\": \"Volume\",
      \"Quantity\": \"Volume\",
      \"Unit\": \"m³\",
      \"Value\": 0.0
    },
    {
      \"RecordIndex\": 2,
      \"HeaderRaw\": \"04933B\",
      \"DIF\": \"0x04\",
      \"VIF\": \"0x93\",
      \"Name\": \"Volume Accumulation (Forward Flow)\",
      \"Quantity\": \"Volume\",
      \"Unit\": \"m³\",
      \"Value\": 12.45
    }
  ]
}
\`\`\`

---

## HeaderRaw Reference Map

When mapping outputs in your downstream Java application, match against \`HeaderRaw\`:

| HeaderRaw | Description |
|---|---|
| \`046D\` | Date and Time string (ISO8601) |
| \`0413\` | Standard Volume (m³) |
| \`04933B\` | Forward Flow Volume Accumulation (m³) |
| \`04933C\` | Backward Flow Volume Accumulation (m³) |
| \`023B\` | Volume Flow Rate (m³/h) |
| \`0259\` | Flow Temperature (°C) |
| \`01FD74\` | Remaining Battery Life (Days) |

---

## Extending for Other OMS Payloads

This microservice is modular and can be extended to support new OMS/M-Bus meters (e.g., Gas, Heat, Electricity, or custom Water meter frames) without breaking existing parser contracts.

### Step 1: Add New VIF/VIFE Extension Codes
In \`src/parser/vif.rs\` (or your VIF decoder module), add match arms inside \`decode_vif\` to register new fields:

\`\`\`rust
pub fn decode_vif(vif: u8, vife_chain: &[u8]) -> VifInfo {
    match vif {
        // Example: Adding Energy / Heat (kWh) support
        0x00..=0x07 => VifInfo {
            name: \"Energy\",
            unit: \"Wh\",
            quantity: \"Energy\",
            multiplier: 10.0f64.powi((vif & 0x07) as i32 - 3),
        },
        // Example: Extension table check (0xFD)
        0xFD => match vife_chain.get(0) {
            Some(0x17) => VifInfo {
                name: \"Error Flags\",
                unit: \"Bitmask\",
                quantity: \"StatusAndDiagnostics\",
                multiplier: 1.0,
            },
            // Add custom manufacturer or OMS extensions here
            _ => VifInfo::unknown(),
        },
        _ => VifInfo::unknown(),
    }
}
\`\`\`

### Step 2: Handle Multi-Byte Header Extensions (\`HeaderRaw\`)
To ensure downstream Java consumers can target new fields explicitly:
1. Ensure the byte-slicing logic in your reader collects all bytes in the DIF/DIFE and VIF/VIFE chain.
2. Format the byte array as an uppercase hexadecimal string assigned to \`HeaderRaw\`:

\`\`\`rust
let header_raw = format_bytes_to_hex(&header_bytes); // e.g., [0x04, 0x93, 0x3B] -> \"04933B\"
\`\`\`

### Step 3: Support Custom Device Link Layers (DLL)
If adding new physical meter types (e.g., Gas or Electricity), update \`decode_dll\` to map the M-Bus Device Type byte (CI Field / Header):

| CI / Device Type Byte | Device Category |
|---|---|
| \`0x02\` | Electricity Meter |
| \`0x03\` | Gas Meter |
| \`0x04\` | Heat Meter |
| \`0x07\` | Water Meter |

---

## Building and Packaging

Cumulocity microservices require a **Linux x86_64 Docker container** packaged inside a \`.zip\` archive alongside a \`cumulocity.json\` manifest.

### Prerequisites
- [Rust & Cargo](https://www.rust-lang.org/)
- [Docker](https://www.docker.com/)
- \`zip\` utility

---

### Step 1: Create the \`cumulocity.json\` Manifest
Ensure a \`cumulocity.json\` manifest file exists in your project root:

\`\`\`json
{
  \"apiVersion\": \"2\",
  \"version\": \"1.0.0\",
  \"name\": \"c8y-oms-parser\",
  \"contextPath\": \"c8y-oms-parser\",
  \"isolation\": \"MULTI_TENANT\",
  \"requiredRoles\": [],
  \"roles\": [],
  \"livenessProbe\": {
    \"httpGet\": {
      \"path\": \"/health\",
      \"port\": 80
    },
    \"initialDelaySeconds\": 10,
    "periodSeconds\": 10
  },
  \"readinessProbe\": {
    \"httpGet\": {
      \"path\": \"/health\",
      \"port\": 80
    },
    \"initialDelaySeconds\": 5,
    \"periodSeconds\": 5
  }
}
\`\`\`

---

### Step 2: Build Local Docker Image
Build the container using multi-stage builds for small binary footprints:

\`\`\`bash
docker build --platform linux/amd64 -t c8y-oms-parser:latest .
\`\`\`

---

### Step 3: Package for Cumulocity IoT

Cumulocity expects the Docker image saved as \`image.tar\` zipped together with \`cumulocity.json\`:

\`\`\`bash
# 1. Export the Docker image as a tarball
docker save c8y-oms-parser:latest -o image.tar

# 2. Compress image.tar and cumulocity.json into a deployable zip file
zip c8y-oms-parser.zip image.tar cumulocity.json

# 3. Clean up the intermediary tar archive
rm image.tar
\`\`\`

---

## Deployment to Cumulocity

1. Log into your **Cumulocity IoT Tenant**.
2. Go to **Administration** -> **Ecosystem** -> **Microservices**.
3. Click **Add Microservice** and upload \`c8y-oms-parser.zip\`.
4. Verify that the health status turns **Green / UP**.
'''
