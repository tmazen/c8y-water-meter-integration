# OMS Decoder Java Microservice (`oms-decoder-microservice-java`)

A Cumulocity Java microservice implementing the **LwM2M Custom Decoder interface** (`com.cumulocity.microservice.customdecoders.api.service.DecoderService`) as demonstrated in Cumulocity's [`sample-lwm2m-custom-decoder`](https://github.com/Cumulocity-IoT/cumulocity-examples/tree/develop/sample-lwm2m-custom-decoder) example.

This microservice processes wM-Bus / OMS telemetry sent via LwM2M from the **WEPTECH SAWAN3 Gateway**. It intercepts LwM2M hex payloads, proxies them to the Rust sidecar (`c8y-oms-parser`) for DIF/VIF parsing, maps telemetry using exact `HeaderRaw` identifiers, and returns a structured `DecoderResult` directly to the Cumulocity LwM2M Agent.

---

## Architecture & Integration Flow

```
+-----------------------+     LwM2M / OMS      +-----------------------------+
| WEPTECH SAWAN3 GW     | -------------------> |    Cumulocity LwM2M Agent   |
| (wM-Bus Meters)       |                      +-----------------------------+
+-----------------------+                                     |
                                                              | Invokes DecoderService.decode()
                                                              v
+-----------------------+   HTTP POST /api/v1/parse   +-----------------------------+
| c8y-oms-parser (Rust) | <-------------------------- |   oms-decoder-microservice  |
| (Port 80)             | --------------------------> |    (Custom LwM2M Decoder)   |
+-----------------------+     Parsed JSON Payload     +-----------------------------+
                                                              |
                                                              | Returns DecoderResult 
                                                              | (Measurements & Events)
                                                              v
                                                      +-----------------------------+
                                                      |   Cumulocity LwM2M Agent    |
                                                      +-----------------------------+
```

1. **LwM2M Interception:** The WEPTECH SAWAN3 Gateway forwards raw unencrypted wM-Bus frames via LwM2M. Cumulocity's LwM2M engine delegates payload handling to this service by executing `DecoderService.decode(inputData, deviceId, args)`.
2. **Rust Sidecar Parsing:** The microservice sends the raw Base64 payload to the sidecar microservice (`c8y-oms-parser`).
3. **Field Extraction & Disambiguation:** The microservice processes the JSON response from Rust, using exact `HeaderRaw` string matching (e.g., `04933B` vs `04933C`) to differentiate metrics.
4. **DecoderResult Assembly:** The microservice builds a `DecoderResult` containing `MeasurementRepresentation` objects stamped with meter time and returns it directly to Cumulocity.

---

## Features

- **Standard Cumulocity LwM2M Custom Decoder:** Fully compliant with the `com.cumulocity.microservice.customdecoders.api` framework.
- **WEPTECH SAWAN3 Gateway Support:** Tailored to parse OMS / wM-Bus payload wrappers transported over LwM2M objects/resources.
- **Microservice-to-Microservice Sidecar Proxy:** Calls `c8y-oms-parser` via Cumulocity internal routing.
- **Exact Register Identification:** Uses `HeaderRaw` byte strings to isolate Forward Flow, Backward Flow, Flow Rates, Temperatures, and Battery Diagnostics.
- **Synchronous Meter Time Alignment:** Uses the meter's internal datetime register (`HeaderRaw: "046D"`) for measurement timestamps.

---

## LwM2M Custom Decoder Implementation

The core entrypoint implements Cumulocity's `DecoderService`:

```java
package com.cumulocity.microservice.customdecoders.api.service;

import com.cumulocity.microservice.customdecoders.api.model.DecoderResult;
import com.cumulocity.model.idtype.GId;
import org.springframework.stereotype.Component;
import java.util.Map;

@Component
public class OmsLwm2mDecoderService implements DecoderService {

    private final OmsParserClient omsParserClient;

    public OmsLwm2mDecoderService(OmsParserClient omsParserClient) {
        this.omsParserClient = omsParserClient;
    }

    @Override
    public DecoderResult decode(String inputData, GId deviceId, Map<String, String> args) 
            throws DecoderServiceException {
        
        // 1. Send Base64 payload to Rust sidecar (c8y-oms-parser)
        OmsParseResponse response = omsParserClient.parsePayload(inputData);

        // 2. Build measurements from parsed records based on HeaderRaw
        DecoderResult result = new DecoderResult();
        
        // 3. Populate Measurements using HeaderRaw mapping logic...
        
        return result;
    }
}
```

---

## Measurement Mapping Matrix

| HeaderRaw | Metric Description | Measurement Fragment / Series | Unit |
|---|---|---|---|
| `046D` | Meter Date & Time | Applied as `DateTime` for all measurements | ISO8601 |
| `0413` | Standard Volume | `Meter_Volume.V` | m³ |
| `023B` | Volume Flow Rate / Power | `Meter_Volume_Flow.Flow` | m³/h |
| `0259` | Flow Temperature | `Meter_Flow_Temperature.T` | °C |
| `01FD74` | Remaining Battery Life | `Meter_Remaining_Battery.Remaining_Battery` | day(s) |

---

## Supported Water Meters

The microservice decodes wireless M-Bus / OMS telemetry payloads sent by LwM2M gateways (such as the **WEPTECH SAWAN3**) for the following supported meters:

| Manufacturer | Model / Series | Protocol / Mode | Key Telemetry Supported |
| :--- | :--- | :--- | :--- |
| **Axioma Metering** | **Qalcosonic W1 | wM-Bus / OMS (868 MHz) | Volume, Flow Rate, Flow/Return Temp, Battery, Error Flags |
| **Generic OMS** | **OMS-compliant Meters** | EN 13757-3 / OMS Spec | Energy, Volume, Mass, Power, Pressure, Date/Time |

---

### Meter-Specific Payload Mapping

#### Axioma Qalcosonic Series
* **Data Registers**: Handles standard volume accumulation as well as directional flow registers (`0x13 0x3B` for Forward Flow, `0x13 0x3C` for Backward Flow).
* **Diagnostics**: Correctly decodes extended diagnostic codes like **Remaining Battery Life** (`0xFD 0x74`) and **Device Status Bitmask** (`0xFD 0x17`).
* **LwM2M Object Mapping**: Telemetry is mapped into standardized Cumulocity IoT measurements automatically via the Rust sidecar parser.

  
---
## Building and Packaging

### Step 1: Maven Build
```bash
mvn clean package -DskipTests
```

---

### Step 2: Build Docker Image
```bash
docker build --platform linux/amd64 -t oms-decoder-microservice-java:latest .
```

---

### Step 3: Package Microservice for Cumulocity

Ensure `cumulocity.json` contains required roles:

```json
{
  "apiVersion": "2",
  "version": "@project.version@",
  "provider": {
    "name": "Cumulocity"
  },
  "isolation": "MULTI_TENANT",
  "replicas": 2,
  "livenessProbe": {
    "httpGet": {
      "path": "/health"
    },
    "initialDelaySeconds": 60
  },
  "readinessProbe": {
    "httpGet": {
      "path": "/health"
    },
    "initialDelaySeconds": 60
  },
  "requiredRoles": [
    "ROLE_INVENTORY_READ",
    "ROLE_IDENTITY_READ",
    "ROLE_MEASUREMENT_ADMIN",
    "ROLE_EVENT_ADMIN"
  ],
  "isDecoder": {
    "name":"Cumulocity OMS Parser"
  }
}
```

Export and zip the package:

```bash
# 1. Export docker image tarball
docker save oms-decoder-microservice-java:latest -o image.tar

# 2. Zip with manifest
zip oms-decoder-microservice-java.zip image.tar cumulocity.json

# 3. Clean up tarball
rm image.tar
```

---

## Deployment & Device Protocol Registration

1. Upload **`oms-decoder-microservice-java.zip`** to Cumulocity (**Administration -> Microservices**).
2. Go to **Device Management -> Device Types -> Device Protocols -> LwM2M**.
3. Create/Edit the LwM2M protocol for the **WEPTECH SAWAN3 Gateway**.
4. In the resource mapping action configuration, select **Custom Decoder Microservice** and choose `Cumulocity OMS Parser`.
