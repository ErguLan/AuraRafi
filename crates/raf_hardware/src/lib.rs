//! # raf_hardware
//!
//! Hardware integration layer for AuraRafi.
//!
//! Provides abstraction for communicating with external hardware
//! devices: serial ports (Arduino, ESP32, custom MCUs), sensors,
//! actuators, and future ML/robot control interfaces.
//!
//! ## Architecture
//!
//! - `serial_session` - The live bounded serial worker, JSON-line protocol,
//!   device identity, telemetry, output receipts and OTA capability discovery
//! - `sensor` - Sensor data model (temperature, distance, voltage, etc.)
//! - `actuator` - Output control (motors, servos, relays, LEDs)
//! - `robot` - High-level robot control interface (future)
//! - `ml` - ML/AI training data export and inference bridge (future)

pub mod actuator;
pub mod ml;
pub mod robot;
pub mod sensor;
pub mod serial_session;

pub use actuator::{ActuatorCommand, ActuatorType};
pub use sensor::{SensorData, SensorType};
