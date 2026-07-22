# c8y-oms-parser Microservice

A high-performance Rust-based microservice designed for Cumulocity IoT. This service receives raw wireless M-Bus (wM-Bus) and OMS (Open Metering System) hex payloads from upstream Java services or external webhooks, decodes the DIF/VIF records, and returns structured data fields.

---

## Architecture & Integration Flow
+--------------------------+       HTTP POST /decode        +-------------------------+
|  Cumulocity Java Service | -----------------------------> |  c8y-oms-parser (Rust)  |
|  (Or Internal Proxy)     | <----------------------------- |  (Port 80)              |
+--------------------------+    Decoded JSON Payload        +-------------------------+

1. The **Java Microservice** sends a raw hexadecimal telemetry frame to the Rust microservice via Cumulocity's internal sidecar proxy (\`http://cumulocity:8111/service/c8y-oms-parser/decode\` or direct \`http://localhost:80/decode\`).
2. The **Rust Microservice** parses the data frame and extracts individual measurement registers along with \`HeaderRaw\` (DIF+VIF+VIFE bytes), \`RecordIndex\`, values, and units.
3. The response is returned as a lightweight JSON object to be processed into Cumulocity \`MeasurementRepresentation\` objects.

---

## Features

- **DIF/VIF Parsing:** Decodes standard M-Bus data structures (Volume, Flow, Temperatures, Time, Battery Status).
- **Exact Field Tracking:** Exposes \`HeaderRaw\` and \`RecordIndex\` for every measurement, allowing client applications to distinguish between identical quantities (e.g., Forward Flow vs. Backward Flow volume).
- **Low Footprint:** Built with Rust for minimal memory consumption and rapid execution under heavy loads.

---

## Usage & API Reference

### Health Check
Cumulocity uses this endpoint for liveness and readiness probes.

* **URL:** \`GET /health\`
* **Response:** \`200 OK\`

\`\`\`json
{
  \"status\": \"UP\"
}
\`\`\`

---

