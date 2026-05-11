use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use winit::event_loop::EventLoop;

mod app;
mod font;
mod gpu_renderer;
mod input;
mod map;
mod model;
mod net;
mod render_assembly;
mod simulation;
mod texture;

use app::App;
use map::load_embedded_map;
use model::PickupKind;
use net::bot::WaypointBot;
use net::client::LocalClient;
use net::server::Server;

use crate::net::bot::{AStarBot, Waypoint};

fn main() {
    let map = Arc::new(load_embedded_map());
    let texture_manager = texture::TextureManager::load();

    let mut server = Server::new(Arc::clone(&map));

    // Human player
    const HUMAN_ID: u64 = 1;
    let input_queue = Arc::new(Mutex::new(VecDeque::new()));
    let local_client = LocalClient::new(
        HUMAN_ID,
        simulation::GameState::new(),
        Arc::clone(&input_queue),
        Arc::clone(&map),
    );
    server.add_client(Box::new(local_client), 21.0, 11.0);

    let bot = AStarBot::new(HUMAN_ID + 1, Arc::clone(&map));
    
    server.add_client(Box::new(bot), 18.0, 11.0);

    // server.spawn_static_prop(18.5, 9.5);
    server.spawn_pickup(15.5, 11.0, PickupKind::Medkit);

    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new(server, input_queue, HUMAN_ID, texture_manager);
    event_loop.run_app(&mut app).unwrap();
}

