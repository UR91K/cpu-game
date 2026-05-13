use std::sync::Arc;

use crate::model::Level;
use crate::net::server::Server;
use crate::simulation::{GameState, TICK_RATE};

pub trait FixedStepSystem {
    fn step(&mut self, dt: f64);
}

pub struct FixedStepSlot<T> {
    step_dt: f64,
    accumulator: f64,
    system: Option<T>,
}

impl<T> FixedStepSlot<T> {
    pub fn from_hz(rate_hz: f64) -> Self {
        assert!(rate_hz > 0.0, "fixed-step rate must be positive");
        Self {
            step_dt: 1.0 / rate_hz,
            accumulator: 0.0,
            system: None,
        }
    }

    pub fn attach(&mut self, system: T) -> Option<T> {
        self.accumulator = 0.0;
        self.system.replace(system)
    }

    #[allow(dead_code)]
    pub fn detach(&mut self) -> Option<T> {
        self.accumulator = 0.0;
        self.system.take()
    }

    #[allow(dead_code)]
    pub fn is_attached(&self) -> bool {
        self.system.is_some()
    }

    pub fn system(&self) -> Option<&T> {
        self.system.as_ref()
    }

    #[allow(dead_code)]
    pub fn system_mut(&mut self) -> Option<&mut T> {
        self.system.as_mut()
    }
}

impl<T: FixedStepSystem> FixedStepSlot<T> {
    pub fn advance(&mut self, frame_dt: f64) -> u32 {
        if frame_dt <= 0.0 || self.system.is_none() {
            return 0;
        }

        self.accumulator += frame_dt;

        let mut steps = 0;
        while self.accumulator >= self.step_dt {
            if let Some(system) = self.system.as_mut() {
                system.step(self.step_dt);
                steps += 1;
            }
            self.accumulator -= self.step_dt;
        }

        steps
    }
}

impl FixedStepSystem for Server {
    fn step(&mut self, dt: f64) {
        self.tick(dt);
    }
}

pub struct ClockManager {
    level: Arc<Level>,
    server: FixedStepSlot<Server>,
}

impl ClockManager {
    pub fn new(level: Arc<Level>) -> Self {
        Self {
            level,
            server: FixedStepSlot::from_hz(TICK_RATE as f64),
        }
    }

    pub fn with_server(level: Arc<Level>, server: Server) -> Self {
        let mut manager = Self::new(level);
        manager.attach_server(server);
        manager
    }

    pub fn advance(&mut self, frame_dt: f64) {
        self.server.advance(frame_dt);
    }

    pub fn attach_server(&mut self, server: Server) -> Option<Server> {
        self.server.attach(server)
    }

    #[allow(dead_code)]
    pub fn detach_server(&mut self) -> Option<Server> {
        self.server.detach()
    }

    #[allow(dead_code)]
    pub fn has_server(&self) -> bool {
        self.server.is_attached()
    }

    pub fn server(&self) -> Option<&Server> {
        self.server.system()
    }

    #[allow(dead_code)]
    pub fn server_mut(&mut self) -> Option<&mut Server> {
        self.server.system_mut()
    }

    pub fn server_state(&self) -> Option<&GameState> {
        self.server().map(|server| &server.state)
    }

    pub fn level(&self) -> &Level {
        self.level.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::{FixedStepSlot, FixedStepSystem};

    #[derive(Default)]
    struct CounterSystem {
        ticks: u32,
        elapsed: f64,
    }

    impl FixedStepSystem for CounterSystem {
        fn step(&mut self, dt: f64) {
            self.ticks += 1;
            self.elapsed += dt;
        }
    }

    #[test]
    fn fixed_step_slot_runs_whole_steps_only() {
        let mut slot = FixedStepSlot::from_hz(4.0);
        slot.attach(CounterSystem::default());

        assert_eq!(slot.advance(0.10), 0);
        assert_eq!(slot.advance(0.20), 1);

        let system = slot.system().unwrap();
        assert_eq!(system.ticks, 1);
        assert!((system.elapsed - 0.25).abs() < 1e-9);
    }

    #[test]
    fn detaching_resets_accumulator() {
        let mut slot = FixedStepSlot::from_hz(2.0);
        slot.attach(CounterSystem::default());

        assert_eq!(slot.advance(0.30), 0);
        slot.detach();
        slot.attach(CounterSystem::default());
        assert_eq!(slot.advance(0.30), 0);
    }
}