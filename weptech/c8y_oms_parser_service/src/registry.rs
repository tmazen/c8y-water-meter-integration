//Responsible for parsing the DLL header, performing decryption via the SecurityEngine, and resolving payload routing to registered meter drivers.

// src/registry.rs

use crate::traits::{DllHeader, MeterDriver, ParseError, ProcessResult};

// Drivers
use crate::drivers::axioma::AxiomaDriver;
use crate::drivers::diehl::DiehlDriver;
use crate::drivers::meter_x::MeterXDriver;

pub struct DriverRegistry {
    drivers: Vec<Box<dyn MeterDriver>>,
}

impl DriverRegistry {
    pub fn new() -> Self {
        Self {
            drivers: Vec::new(),
        }
    }

    pub fn register(&mut self, driver: Box<dyn MeterDriver>) {
        self.drivers.push(driver);
    }

    pub fn process(
        &self,
        raw_payload: &[u8],
        oms_mode: Option<u8>,
        key: Option<&[u8]>,
    ) -> Result<ProcessResult, ParseError> {
        let header = DllHeader::from_bytes(raw_payload)?;

        for driver in &self.drivers {
            if driver.supports(&header) {
                return driver.parse(raw_payload, oms_mode, key);
            }
        }

        Err(ParseError::NoMatchingDriver {
            manufacturer: header.manufacturer_code(),
            device_type: header.device_type,
            version: header.version,
        })
    }
}

impl Default for DriverRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn build_driver_registry() -> DriverRegistry {
    let mut registry = DriverRegistry::new();

    // Register supported drivers using standard ::new() constructors
    registry.register(Box::new(AxiomaDriver::new()));
    registry.register(Box::new(DiehlDriver::new()));
    registry.register(Box::new(MeterXDriver::new()));

    registry
}