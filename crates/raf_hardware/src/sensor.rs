//! Sensor data model.
//!
//! Defines sensor types and the data they produce. Sensors read from
//! hardware devices through the serial interface and provide data
//! to the engine (for circuits, games, or ML training).

use serde::{Deserialize, Serialize};

/// Type of sensor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SensorType {
    /// Temperature sensor (Celsius).
    Temperature,
    /// Humidity sensor (percent).
    Humidity,
    /// Distance/proximity sensor (centimeters).
    Distance,
    /// Light level (lux).
    Light,
    /// Voltage reading (volts).
    Voltage,
    /// Current reading (amperes).
    Current,
    /// Accelerometer (g-force on 3 axes).
    Accelerometer,
    /// Gyroscope (degrees/sec on 3 axes).
    Gyroscope,
    /// Magnetic field (microTesla).
    MagneticField,
    /// Pressure (Pascals).
    Pressure,
    /// Generic analog value (0.0 - 1.0).
    Analog,
    /// Digital on/off state.
    Digital,
    /// Custom sensor with user-defined label.
    Custom,
}

impl SensorType {
    /// Human-readable name.
    pub fn display_name(&self) -> &str {
        match self {
            Self::Temperature => "Temperature",
            Self::Humidity => "Humidity",
            Self::Distance => "Distance",
            Self::Light => "Light",
            Self::Voltage => "Voltage",
            Self::Current => "Current",
            Self::Accelerometer => "Accelerometer",
            Self::Gyroscope => "Gyroscope",
            Self::MagneticField => "Magnetic Field",
            Self::Pressure => "Pressure",
            Self::Analog => "Analog",
            Self::Digital => "Digital",
            Self::Custom => "Custom",
        }
    }

    /// Unit label for display.
    pub fn unit(&self) -> &str {
        match self {
            Self::Temperature => "C",
            Self::Humidity => "%",
            Self::Distance => "cm",
            Self::Light => "lux",
            Self::Voltage => "V",
            Self::Current => "A",
            Self::Accelerometer => "g",
            Self::Gyroscope => "deg/s",
            Self::MagneticField => "uT",
            Self::Pressure => "Pa",
            Self::Analog => "",
            Self::Digital => "",
            Self::Custom => "",
        }
    }
}

/// A single sensor reading.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorData {
    /// Sensor identifier (user-defined label or auto-assigned).
    pub name: String,
    /// Sensor type.
    pub sensor_type: SensorType,
    /// Current value (primary axis or scalar).
    pub value: f64,
    /// Secondary values for multi-axis sensors (x, y, z).
    pub axes: Option<[f64; 3]>,
    /// Timestamp in milliseconds.
    pub timestamp_ms: u64,
    /// Whether this reading is valid / within expected range.
    pub valid: bool,
}

impl Default for SensorData {
    fn default() -> Self {
        Self {
            name: String::new(),
            sensor_type: SensorType::Analog,
            value: 0.0,
            axes: None,
            timestamp_ms: 0,
            valid: true,
        }
    }
}
