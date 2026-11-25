//! Arbitration uses live gameplay-owned resources without changing their ownership.

use raf_core::scene::{
    animation::{ActionLayer, AnimationLayer},
    SceneNode, VariableValue,
};

const RELEASE_TIME: f32 = 0.10;

#[derive(Clone, Copy)]
struct Playing {
    layer: usize,
    elapsed: f32,
}

#[derive(Default)]
pub(super) struct ActionPlayback {
    active: Option<Playing>,
    releasing: Option<(Playing, f32)>,
    last_request: String,
    completed: bool,
}

impl ActionPlayback {
    pub(super) fn step(&mut self, actor: Option<&SceneNode>, layers: &[AnimationLayer], dt: f32) {
        let Some(actor) = actor else {
            return;
        };
        let request = match actor.get_variable("animation_action") {
            Some(VariableValue::Text(value)) => value.as_str(),
            _ => "",
        };
        if self.last_request != request {
            self.last_request.clear();
            self.last_request.push_str(request);
            self.completed = false;
        }
        if let Some((_, remaining)) = &mut self.releasing {
            *remaining -= dt;
            if *remaining <= 0.0 {
                self.releasing = None;
            }
        }
        let available = |action: &ActionLayer| {
            action
                .free_slots
                .iter()
                .all(|slot| matches!(actor.get_variable(slot), Some(VariableValue::Entity(None))))
        };
        if let Some(active) = &mut self.active {
            let layer = &layers[active.layer];
            let action = layer.action.as_ref().expect("active layer is an action");
            active.elapsed += dt;
            if active.elapsed >= layer.duration {
                self.active = None;
                self.completed = request == action.request;
            } else if !available(action) || (action.interruptible && request != action.request) {
                self.releasing = self.active.take().map(|play| (play, RELEASE_TIME));
            }
        }
        if self.active.is_none() && !self.completed && !request.is_empty() {
            let choice = layers
                .iter()
                .enumerate()
                .filter_map(|(index, layer)| {
                    let action = layer.action.as_ref()?;
                    (action.request == request && available(action))
                        .then_some((index, action.priority))
                })
                .max_by_key(|&(index, priority)| (priority, std::cmp::Reverse(index)));
            self.active = choice.map(|(layer, _)| Playing {
                layer,
                elapsed: 0.0,
            });
        }
    }

    pub(super) fn sample(&self, index: usize, layer: &AnimationLayer) -> Option<(f32, f32)> {
        if let Some(active) = self.active.filter(|play| play.layer == index) {
            return layer.sample_at(f64::from(active.elapsed));
        }
        self.releasing
            .filter(|(play, _)| play.layer == index)
            .and_then(|(play, remaining)| {
                layer
                    .sample_at(f64::from(play.elapsed))
                    .map(|(time, weight)| (time, weight * (remaining / RELEASE_TIME)))
            })
    }
}
