//! Opt-in per-attachment scheduling; fixed/late physics and camera hooks stay precise.
use rhai::{Dynamic, Map, Scope};

#[derive(Default)]
pub(super) struct UpdateSchedule {
    accumulated: f32,
}
impl UpdateSchedule {
    pub fn due(&mut self, scope: &Scope<'_>, dt: f32) -> Result<Option<f32>, String> {
        let (enabled, interval) = if let Some(config) = config(scope)? {
            let enabled = match config.get("enabled") {
                Some(value) => value
                    .clone()
                    .try_cast::<bool>()
                    .ok_or("behavior.enabled requires a boolean")?,
                None => true,
            };
            let interval = match config.get("update_interval") {
                Some(value) if value.is::<rhai::INT>() => value.clone().cast::<rhai::INT>() as f64,
                Some(value) => value
                    .clone()
                    .try_cast::<f64>()
                    .ok_or("behavior.update_interval requires seconds")?,
                None => 0.0,
            };
            if !interval.is_finite() || !(0.0..=60.0).contains(&interval) {
                return Err("behavior.update_interval requires 0..60 finite seconds".into());
            }
            (enabled, interval as f32)
        } else {
            (true, 0.0)
        };
        if !enabled {
            self.accumulated = 0.0;
            return Ok(None);
        }
        self.accumulated = (self.accumulated + dt).min(60.0);
        if self.accumulated + f32::EPSILON < interval {
            return Ok(None);
        }
        let elapsed = std::mem::take(&mut self.accumulated);
        Ok(Some(elapsed))
    }
}

fn config(scope: &Scope<'_>) -> Result<Option<Map>, String> {
    match scope.get_value::<Dynamic>("behavior") {
        Some(value) => value
            .try_cast::<Map>()
            .map(Some)
            .ok_or_else(|| "behavior configuration requires a map".into()),
        None => Ok(None),
    }
}

pub(super) fn accepts_event(scope: &Scope<'_>, name: &str) -> Result<bool, String> {
    let Some(config) = config(scope)? else {
        return Ok(true);
    };
    let Some(events) = config.get("events") else {
        return Ok(true);
    };
    let events = events
        .clone()
        .try_cast::<rhai::Array>()
        .ok_or("behavior.events requires an array of event names")?;
    if events.len() > 64 {
        return Err("behavior.events exceeds 64 subscriptions".into());
    }
    let mut matches = false;
    for event in events {
        let event = event
            .try_cast::<rhai::ImmutableString>()
            .ok_or("behavior.events requires string names")?;
        if event.is_empty() || event.len() > 128 {
            return Err("invalid behavior event name".into());
        }
        matches |= event.as_str() == name;
    }
    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn behavior_schedule_sleep_resume_and_invalid_configuration_are_bounded() {
        let mut scope = Scope::new();
        let mut config = Map::new();
        config.insert("enabled".into(), false.into());
        config.insert("update_interval".into(), Dynamic::from_float(0.5));
        config.insert("events".into(), Dynamic::from_array(vec!["wake".into()]));
        scope.push("behavior", config.clone());
        let mut schedule = UpdateSchedule::default();
        assert_eq!(schedule.due(&scope, 2.0).unwrap(), None);
        assert!(accepts_event(&scope, "wake").unwrap());
        assert!(!accepts_event(&scope, "other").unwrap());
        config.insert("enabled".into(), true.into());
        scope.set_value("behavior", config.clone());
        assert_eq!(schedule.due(&scope, 0.25).unwrap(), None);
        assert_eq!(schedule.due(&scope, 0.25).unwrap(), Some(0.5));
        config.insert("update_interval".into(), Dynamic::from_float(f64::NAN));
        scope.set_value("behavior", config);
        assert!(schedule.due(&scope, 0.1).is_err());
    }
}
