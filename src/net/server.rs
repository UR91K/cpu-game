#![allow(unused)]
use std::sync::Arc;

use super::Controller;
use crate::model::{ControllerId, EntityId, Level, PickupKind};
use crate::simulation::{GameState, Player, tick};

pub struct Server {
    pub state: GameState,
    pub level: Arc<Level>,
    controllers: Vec<Box<dyn Controller>>,
    next_controller_id: u64,
}

impl Server {
    pub fn new(level: Arc<Level>) -> Self {
        Self {
            state: GameState::new(),
            level,
            controllers: Vec::new(),
            next_controller_id: 1,
        }
    }

    /// add a controller and spawn its player at the given position
    pub fn add_controller(
        &mut self,
        mut controller: Box<dyn Controller>,
        spawn_x: f64,
        spawn_y: f64,
    ) {
        let id = controller.id();
        let pawn_id = self.state.spawn_pawn(spawn_x, spawn_y, Some(id));
        self.state.players.insert(id, Player::new(pawn_id));
        // send initial state so the client is aware of the world
        controller.receive_state(&self.state);
        self.controllers.push(controller);
    }

    /// gather all client inputs, advance the simulation by the delta, and
    /// push the authoritative state back to every client.
    pub fn tick(&mut self, delta: f64) {
        let mut inputs = Vec::new();
        for controller in &mut self.controllers {
            inputs.extend(controller.poll_inputs());
        }
        self.state = tick(&self.state, &inputs, &self.level, delta);
        for controller in &mut self.controllers {
            controller.receive_state(&self.state);
        }
    }

    pub fn remove_controller(&mut self, id: ControllerId) {
        self.controllers.retain(|c| c.id() != id);
        if let Some(player) = self.state.players.remove(&id) {
            self.state.remove_entity(player.pawn_id);
        }
    }

    pub fn teleport_pawn(&mut self, id: ControllerId, x: f64, y: f64) -> Option<()> {
        let pawn_id = self.state.players.get(&id)?.pawn_id;
        self.state.teleport_entity(pawn_id, x, y)
    }

    pub fn teleport_entity(&mut self, entity_id: EntityId, x: f64, y: f64) -> Option<()> {
        self.state.teleport_entity(entity_id, x, y)
    }

    pub fn spawn_static_prop(&mut self, x: f64, y: f64) {
        self.state.spawn_static_prop(x, y);
    }

    pub fn spawn_pickup(&mut self, x: f64, y: f64, pickup_kind: PickupKind) {
        self.state.spawn_pickup(x, y, pickup_kind);
    }

    pub fn spawn_wanderer(&mut self, id: ControllerId, x: f64, y: f64) {
        let bot =
            crate::net::bots::wandering::WanderingController::new(id, Arc::clone(&self.level));
        self.add_controller(Box::new(bot), x, y);
    }

    pub fn allocate_controller_id(&mut self) -> u64 {
        while self.state.players.contains_key(&self.next_controller_id) {
            self.next_controller_id += 1;
        }
        let id = self.next_controller_id;
        self.next_controller_id += 1;
        id
    }
}

pub fn build_headless_server(level: Arc<Level>) -> Server {
    let mut server = Server::new(level);
    populate_demo_world(&mut server);
    server
}

fn populate_demo_world(server: &mut Server) {
    server.spawn_wanderer(2, 18.0, 11.0);
    server.spawn_pickup(15.5, 11.0, PickupKind::Medkit);
}
