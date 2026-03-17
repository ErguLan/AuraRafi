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
//! - `serial` - Serial port abstraction (COM/ttyUSB), message protocol
//! - `sensor` - Sensor data model (temperature, distance, voltage, etc.)
//! - `actuator` - Output control (motors, servos, relays, LEDs)
//! - `robot` - High-level robot control interface (future)
//! - `ml` - ML/AI training data export and inference bridge (future)

pub mod serial;
pub mod sensor;
pub mod actuator;
pub mod robot;
pub mod ml;

pub use serial::{SerialConfig, SerialMessage, SerialPort};
pub use sensor::{SensorData, SensorType};
pub use actuator::{ActuatorCommand, ActuatorType};
