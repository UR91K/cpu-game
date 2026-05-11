use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use winit::event_loop::EventLoop;

mod app;
mod font;
mod input;
mod level;
mod model;
mod net;
mod render_assembly;
mod renderer;
mod simulation;
mod text_layer;
mod texture;

use app::App;
use level::load_embedded_level;
use model::PickupKind;
use net::local_controller::LocalController;
use net::server::Server;

fn main() {
    let level = Arc::new(load_embedded_level());
    let texture_manager = texture::TextureManager::load();

    let mut server = Server::new(Arc::clone(&level));

    // Human player
    const HUMAN_ID: u64 = 1;
    let input_queue = Arc::new(Mutex::new(VecDeque::new()));
    let local_client = LocalController::new(
        HUMAN_ID,
        simulation::GameState::new(),
        Arc::clone(&input_queue),
        Arc::clone(&level),
    );

    server.add_controller(Box::new(local_client), 21.0, 11.0);

    server.spawn_wanderer(2, 18.0, 11.0);

    // server.spawn_static_prop(18.5, 9.5);
    server.spawn_pickup(15.5, 11.0, PickupKind::Medkit);

    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new(server, input_queue, HUMAN_ID, texture_manager);
    event_loop.run_app(&mut app).unwrap();
}
