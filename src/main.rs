use std::env;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
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
use input::{ChannelInputSink, InputSink};
use level::load_embedded_level;
use model::{Level, PickupKind};
use net::channel_controller::ChannelController;
use net::server::Server;
use runtime::{ChannelClientRuntime, GameRuntime};
use simulation::TICK_DT;

const DEFAULT_PORT: u16 = 3456;

struct ClientLaunchOptions {
    server_ip: Option<String>,
    server_port: u16,
}

struct ServerLaunchOptions {
    port: u16,
}

struct HostLaunchOptions {
    port: u16,
}

enum StartupMode {
    Client(ClientLaunchOptions),
    Server(ServerLaunchOptions),
    Host(HostLaunchOptions),
}

impl StartupMode {
    fn from_args() -> Result<Self, String> {
        let mut mode_name = String::from("client");
        let mut server_ip = None;
        let mut port = DEFAULT_PORT;
        let mut args = env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "client" | "--client" | "--mode=client" => mode_name = String::from("client"),
                "server" | "--server" | "--mode=server" => mode_name = String::from("server"),
                "host" | "--host" | "--mode=host" => mode_name = String::from("host"),
                "--ip" => {
                    let Some(value) = args.next() else {
                        return Err(String::from("missing value after --ip"));
                    };
                    server_ip = Some(value);
                }
                "--port" => {
                    let Some(value) = args.next() else {
                        return Err(String::from("missing value after --port"));
                    };
                    port = parse_port(&value)?;
                }
                "--help" | "-h" => {
                    return Err(String::from(
                        "usage: cpu-game [--client|--server|--host|--mode=client|--mode=server|--mode=host] [--ip <addr>] [--port <port>]",
                    ));
                }
                _ => {
                    if let Some(value) = arg.strip_prefix("--ip=") {
                        server_ip = Some(value.to_string());
                    } else if let Some(value) = arg.strip_prefix("--port=") {
                        port = parse_port(value)?;
                    } else {
                        return Err(format!("unrecognized argument: {arg}"));
                    }
                }
            }
        }

        match mode_name.as_str() {
            "client" => Ok(Self::Client(ClientLaunchOptions {
                server_ip,
                server_port: port,
            })),
            "server" => Ok(Self::Server(ServerLaunchOptions { port })),
            "host" => Ok(Self::Host(HostLaunchOptions { port })),
            _ => Err(format!("unsupported launch mode: {mode_name}")),
        }
    }
}

fn parse_port(value: &str) -> Result<u16, String> {
    value
        .parse::<u16>()
        .map_err(|_| format!("invalid port: {value}"))
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
        StartupMode::Client(options) => run_client(options),
        StartupMode::Server(options) => run_server(options),
        StartupMode::Host(options) => run_host(options),
    }
}

fn run_client(options: ClientLaunchOptions) {
    let server_ip = options.server_ip.unwrap_or_else(|| {
        eprintln!("client mode requires --ip <addr>");
        std::process::exit(1);
    });
    let _requested_server_addr = format!("{}:{}", server_ip, options.server_port);
    let level = Arc::new(load_embedded_level());
    let texture_manager = texture::TextureManager::load();

    const HUMAN_ID: u64 = 1;
    let (input_tx, _input_rx) = mpsc::channel();
    let (_update_tx, update_rx) = mpsc::channel();
    let client_runtime = ChannelClientRuntime::new(Arc::clone(&level), update_rx);
    let input_sink = ChannelInputSink::new(input_tx);

    run_windowed_client(Box::new(client_runtime), Box::new(input_sink), HUMAN_ID, texture_manager);
}

fn run_host(options: HostLaunchOptions) {
    let _host_port = options.port;
    let level = Arc::new(load_embedded_level());
    let texture_manager = texture::TextureManager::load();

    const HUMAN_ID: u64 = 1;
    let (input_tx, input_rx) = mpsc::channel();
    let (update_tx, update_rx) = mpsc::channel();

    let server_level = Arc::clone(&level);
    thread::spawn(move || {
        let server = build_channel_host_server(server_level.clone(), input_rx, update_tx, HUMAN_ID);
        let clock_manager = ClockManager::with_server(server_level, server);
        run_fixed_tick_loop(clock_manager);
    });

    let client_runtime = ChannelClientRuntime::new(Arc::clone(&level), update_rx);
    let input_sink = ChannelInputSink::new(input_tx);

    run_windowed_client(Box::new(client_runtime), Box::new(input_sink), HUMAN_ID, texture_manager);
}

fn run_windowed_client(
    runtime: Box<dyn GameRuntime>,
    input_sink: Box<dyn InputSink>,
    human_id: u64,
    texture_manager: texture::TextureManager,
) {
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new(runtime, input_sink, human_id, texture_manager);
    event_loop.run_app(&mut app).unwrap();
}

fn run_server(options: ServerLaunchOptions) {
    let _bind_port = options.port;
    let level = Arc::new(load_embedded_level());
    let server = build_headless_server(Arc::clone(&level));
    let clock_manager = ClockManager::with_server(level, server);
    run_fixed_tick_loop(clock_manager);
}

fn run_fixed_tick_loop(mut clock_manager: ClockManager) {
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

fn build_headless_server(level: Arc<Level>) -> Server {
    let mut server = Server::new(level);
    populate_demo_world(&mut server);
    server
}

fn build_channel_host_server(
    level: Arc<Level>,
    input_rx: mpsc::Receiver<input::InputMessage>,
    update_tx: mpsc::Sender<runtime::AuthoritativeUpdate>,
    human_id: u64,
) -> Server {
    let mut server = Server::new(Arc::clone(&level));
    let controller = ChannelController::new(human_id, input_rx, update_tx);

    server.add_controller(Box::new(controller), 21.0, 11.0);
    populate_demo_world(&mut server);
    server
}

fn populate_demo_world(server: &mut Server) {
    server.spawn_wanderer(2, 18.0, 11.0);
    server.spawn_pickup(15.5, 11.0, PickupKind::Medkit);
}
