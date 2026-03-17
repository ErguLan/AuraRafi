//! Actuator control model.
//!
//! Defines output device types and commands for controlling motors,
//! servos, relays, and other actuators through the serial interface.

use serde::{Deserialize, Serialize};

/// Type of actuator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActuatorType {
    /// DC motor (speed + direction).
    DcMotor,
    /// Servo motor (angle 0-180).
    Servo,
    /// Stepper motor (steps + direction).
    Stepper,
    /// Relay (on/off).
    Relay,
    /// LED (brightness 0-255).
    Led,
    /// Buzzer / speaker (frequency + duration).
    Buzzer,
    /// PWM output (duty cycle 0.0 - 1.0).
    Pwm,
    /// Digital output (high/low).
    DigitalOut,
    /// Custom actuator.
    Custom,
}

impl ActuatorType {
    /// Human-readable name.
    pub fn display_name(&self) -> &str {
        match self {
            Self::DcMotor => "DC Motor",
            Self::Servo => "Servo",
            Self::Stepper => "Stepper Motor",
            Self::Relay => "Relay",
            Self::Led => "LED",
            Self::Buzzer => "Buzzer",
            Self::Pwm => "PWM Output",
            Self::DigitalOut => "Digital Output",
            Self::Custom => "Custom",
        }
    }
}

/// A command to send to an actuator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActuatorCommand {
    /// Actuator identifier.
    pub name: String,
    /// Actuator type.
    pub actuator_type: ActuatorType,
    /// Primary value (speed, angle, duty cycle, frequency, etc.).
    pub value: f64,
    /// Secondary value (duration for buzzer, direction for motor).
    pub secondary: Option<f64>,
    /// Whether the actuator should be enabled.
    pub enabled: bool,
}

impl Default for ActuatorCommand {
    fn default() -> Self {
        Self {
            name: String::new(),
            actuator_type: ActuatorType::DigitalOut,
            value: 0.0,
            secondary: None,
            enabled: false,
        }
    }
}
