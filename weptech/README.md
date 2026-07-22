# WEPTECH SAWAN3 OMS / LwM2M Integration for Cumulocity

This umbrella repository contains the complete ecosystem required to integrate **WEPTECH SAWAN3 Gateways** and connected **OMS / wM-Bus smart meters** into **Cumulocity** using standard LwM2M protocol definitions and custom decoder microservices.

---

## Repository Structure

```text
{b3}text
.
├── LWM2M-XML-Files/
│   └── (LwM2M Object XML definitions for WEPTECH SAWAN3 Gateway)
├── c8y_oms_parser_service/
│   └── (High-performance Rust sidecar microservice for raw OMS payload decoding)
└── oms-decoder-c8y-oms-parser/
    └── oms-decoder-microservice-java/
        └── (Java Spring Boot microservice implementing Lwm2mDecoderService)
{b3}
```
