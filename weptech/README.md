# WEPTECH SAWAN3 OMS / LwM2M Integration for Cumulocity

This umbrella repository contains the complete ecosystem required to integrate **WEPTECH SAWAN3 Gateways** and connected **OMS / wM-Bus smart meters** into **Cumulocity** using standard LwM2M protocol definitions and custom decoder microservices.

---

## Repository Structure


```text
.
├── LWM2M-XML-Files/
│   └── (LwM2M Object XML definitions for WEPTECH SAWAN3 Gateway)
├── c8y_oms_parser_service/
│   └── (High-performance Rust sidecar microservice for raw OMS payload decoding)
└── oms-decoder-c8y-oms-parser/
    └── oms-decoder-microservice-java/
        └── (Java Spring Boot microservice implementing Lwm2mDecoderService)
```
---

## Component Overview

### 1. `LWM2M-XML-Files`
Contains the standard LwM2M XML object and resource specification files for the **WEPTECH SAWAN3 Gateway**. 
* Defines custom objects and resources needed by Cumulocity to understand incoming LwM2M payloads from the gateway.
* Upload these XML files directly to **Cumulocity -> Device Management -> Device Types -> Device Protocols** to register object definitions.

### 2. `c8y_oms_parser_service` (Rust)
A lightweight, fast **Rust** sidecar microservice that handles low-level bitwise parsing of raw wM-Bus / OMS payloads.
* Exposes HTTP endpoint `/api/v1/parse`.
* Decodes Base64/Hex raw telemetry frames into structured JSON containing exact `HeaderRaw` DIF/VIF fields (e.g., `046D` for timestamp, `04933B` for volume, `023B` for flow rate, `0259` for flow temperature).

### 3. `oms-decoder-c8y-oms-parser/oms-decoder-microservice-java` (Java)
A Cumulocity microservice implementing the **`com.cumulocity.microservice.customdecoders.api.service.DecoderService`** interface.
* Intercepts LwM2M payload updates from Cumulocity's LwM2M agent.
* Delegates raw frame decoding to `c8y_oms_parser_service` over internal sidecar routing.
* Maps decoded DIF/VIF records into standard Cumulocity `MeasurementRepresentation` objects.

---

## End-to-End Data Pipeline

```text
+-----------------------+     LwM2M Telemetry      +-----------------------------+
| WEPTECH SAWAN3 GW     | -----------------------> | Cumulocity LwM2M Agent      |
| (wM-Bus Meters)       |                          +-----------------------------+
+-----------------------+                                         |
                                                                  | Invokes DecoderService
                                                                  v
+-----------------------+   HTTP POST /api/v1/parse   +-----------------------------+
| c8y_oms_parser_service| <-------------------------- | oms-decoder-microservice    |
| (Rust Sidecar)        | --------------------------> |  ( LwM2M Custom Decoder)    |
+-----------------------+     Parsed JSON Payload     +-----------------------------+
                                                                  |
                                                                  | Returns DecoderResult
                                                                  v
                                                      +-----------------------------+
                                                      | Cumulocity LwM2M Agent      |
                                                      +-----------------------------+
```

---
