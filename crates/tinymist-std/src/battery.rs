//! Battery-related utilities.

#[cfg(feature = "system")]
mod system {
    /// Detects if the system is currently in power saving mode.
    pub fn is_power_saving() -> bool {
        ::battery::Manager::new()
            .ok()
            .and_then(|manager| manager.batteries().ok())
            .map(|mut batteries| {
                batteries.any(|battery| match battery {
                    Ok(bat) => matches!(bat.state(), ::battery::State::Discharging),
                    Err(_) => false,
                })
            })
            .unwrap_or(false)
    }
}
#[cfg(feature = "system")]
pub use system::*;

#[cfg(not(feature = "system"))]
mod other {
    /// Detects if the system is currently in power saving mode.
    pub fn is_power_saving() -> bool {
        false
    }
}
#[cfg(not(feature = "system"))]
pub use other::*;
