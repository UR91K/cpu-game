use std::sync::Arc;

use crate::model::{Map, PickupKind, PlayerId};
use crate::simulation::{tick, GameState, PlayerState};
use super::Client;

pub struct Server {
    pub state: GameState,
    pub map: Arc<Map>,
    clients: Vec<Box<dyn Client>>,
}

impl Server {
    pub fn new(map: Arc<Map>) -> Self {
        Self {
            state: GameState::new(),
            map,
            clients: Vec::new(),
        }
    }

    /// add a client and spawn its player at the given position
    pub fn add_client(&mut self, mut client: Box<dyn Client>, spawn_x: f64, spawn_y: f64) {
        let id = client.id();
        let actor_id = self.state.spawn_actor(spawn_x, spawn_y, Some(id));
        self.state
            .players
            .insert(id, PlayerState::new(actor_id));
        // send initial state so the client is aware of the world
        client.receive_state(&self.state);
        self.clients.push(client);
    }

    /// gather all client inputs, advance the simulation by the delta, and
    /// push the authoritative state back to every client.
    pub fn tick(&mut self, delta: f64) {
        let mut inputs = Vec::new();
        for client in &mut self.clients {
            inputs.extend(client.poll_inputs());
        }
        self.state = tick(&self.state, &inputs, &self.map, delta);
        for client in &mut self.clients {
            client.receive_state(&self.state);
        }
    }

    pub fn remove_client(&mut self, id: PlayerId) {
        self.clients.retain(|c| c.id() != id);
        if let Some(player) = self.state.players.remove(&id) {
            self.state.remove_object(player.controlled_object);
        }
    }

    pub fn spawn_static_prop(&mut self, x: f64, y: f64) {
        self.state.spawn_static_prop(x, y);
    }

    pub fn spawn_pickup(&mut self, x: f64, y: f64, pickup_kind: PickupKind) {
        self.state.spawn_pickup(x, y, pickup_kind);
    }
}
