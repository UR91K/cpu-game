use std::collections::VecDeque;
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
use net::bot::BotClient;
use net::client::LocalClient;
use net::server::Server;

fn main() {
    let map = Arc::new(load_map("textures/map.png"));
    let textures = texture::load_textures("textures");

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

    // A bot that patrols a simple square route
    let bot = BotClient::new(
        2,
        vec![(21.0, 8.0), (15.0, 8.0), (15.0, 14.0), (21.0, 14.0)],
    );
    server.add_client(Box::new(bot), 15.0, 11.0);

    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new(server, input_queue, HUMAN_ID, textures);
    event_loop.run_app(&mut app).unwrap();
}

