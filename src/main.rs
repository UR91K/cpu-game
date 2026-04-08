use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use winit::event_loop::EventLoop;

mod app;
mod input;
mod map;
mod model;
mod net;
mod renderer;
mod simulation;
mod texture;

use app::App;
use map::load_map;
use net::bot::WaypointBot;
use net::client::LocalClient;
use net::server::Server;

use crate::net::bot::Waypoint;

fn main() {
    let asset_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("textures");
    let map_path = asset_root.join("map.png");

    let map = Arc::new(load_map(&map_path.to_string_lossy()));
    let textures = texture::load_textures(&asset_root.to_string_lossy());

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

    let bot = WaypointBot::new(
        2,
        vec![
            Waypoint { x: 21.0, y: 8.0 },
            Waypoint { x: 15.0, y: 8.0 },
            Waypoint { x: 15.0, y: 14.0 },
            Waypoint { x: 21.0, y: 14.0 },
        ],
    );
    server.add_client(Box::new(bot), 18.0, 11.0);

    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new(server, input_queue, HUMAN_ID, textures);
    event_loop.run_app(&mut app).unwrap();
}

