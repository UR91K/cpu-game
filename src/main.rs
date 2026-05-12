use std::collections::VecDeque;
use std::env;
use std::thread;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use winit::event_loop::EventLoop;

mod app;
mod clock;
mod font;
mod input;
mod level;
mod model;
mod net;
mod render_assembly;
mod renderer;
mod runtime;
mod simulation;
mod text_layer;
mod texture;

use app::App;
use clock::ClockManager;
use input::LocalInputSink;
use level::load_embedded_level;
use model::{Level, PickupKind};
use net::local_controller::LocalController;
use net::server::Server;
use simulation::TICK_DT;

enum StartupMode {
    Client,
    Server,
}

impl StartupMode {
    fn from_args() -> Result<Self, String> {
        let mut mode = Self::Client;

        for arg in env::args().skip(1) {
            match arg.as_str() {
                "client" | "--client" | "--mode=client" => mode = Self::Client,
                "server" | "--server" | "--mode=server" => mode = Self::Server,
                "--help" | "-h" => {
                    return Err(String::from(
                        "usage: cpu-game [--client|--server|--mode=client|--mode=server]",
                    ));
                }
                _ => return Err(format!("unrecognized argument: {arg}")),
            }
        }

        Ok(mode)
    }
}

fn main() {
    let mode = match StartupMode::from_args() {
        Ok(mode) => mode,
        Err(message) => {
            eprintln!("{message}");
            return;
        }
    };

    match mode {
        StartupMode::Client => run_client(),
        StartupMode::Server => run_server(),
    }
}

fn run_client() {
    let level = Arc::new(load_embedded_level());
    let texture_manager = texture::TextureManager::load();

    const HUMAN_ID: u64 = 1;
    let input_queue = Arc::new(Mutex::new(VecDeque::new()));

    let server = build_local_server(Arc::clone(&level), Arc::clone(&input_queue), HUMAN_ID);

    let clock_manager = ClockManager::with_server(Arc::clone(&level), server);
    let input_sink = LocalInputSink::new(input_queue);

    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new(
        Box::new(clock_manager),
        Box::new(input_sink),
        HUMAN_ID,
        texture_manager,
    );
    event_loop.run_app(&mut app).unwrap();
}

fn run_server() {
    let level = Arc::new(load_embedded_level());
    let server = build_headless_server(Arc::clone(&level));
    let mut clock_manager = ClockManager::with_server(level, server);
    let tick_duration = Duration::from_secs_f64(TICK_DT);
    let mut next_tick = Instant::now() + tick_duration;

    loop {
        let now = Instant::now();
        if now < next_tick {
            thread::sleep(next_tick - now);
            continue;
        }

        clock_manager.advance(TICK_DT);
        next_tick += tick_duration;

        let catch_up_limit = now + tick_duration;
        if next_tick < catch_up_limit {
            next_tick = catch_up_limit;
        }
    }
}

fn build_local_server(
    level: Arc<Level>,
    input_queue: Arc<Mutex<VecDeque<input::InputMessage>>>,
    human_id: u64,
) -> Server {
    let mut server = Server::new(Arc::clone(&level));

    let local_client = LocalController::new(
        human_id,
        simulation::GameState::new(),
        input_queue,
        Arc::clone(&level),
    );

    server.add_controller(Box::new(local_client), 21.0, 11.0);
    populate_demo_world(&mut server);
    server
}

fn build_headless_server(level: Arc<Level>) -> Server {
    let mut server = Server::new(level);
    populate_demo_world(&mut server);
    server
}

fn populate_demo_world(server: &mut Server) {
    server.spawn_wanderer(2, 18.0, 11.0);
    server.spawn_pickup(15.5, 11.0, PickupKind::Medkit);
}
