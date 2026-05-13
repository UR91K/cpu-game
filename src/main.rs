use std::env;
use std::net::{TcpListener, TcpStream};
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
mod transport;
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
use transport::{ClientMessage, ServerMessage};

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
    let requested_server_addr = format!("{}:{}", server_ip, options.server_port);
    let level = Arc::new(load_embedded_level());
    let texture_manager = texture::TextureManager::load();

    const HUMAN_ID: u64 = 1;
    let (input_tx, input_rx) = mpsc::channel();
    let (update_tx, update_rx) = mpsc::channel();
    let pending_inputs = Arc::new(std::sync::Mutex::new(Vec::new()));
    start_tcp_client_transport(requested_server_addr, input_rx, update_tx);
    let client_runtime = ChannelClientRuntime::new(
        Arc::clone(&level),
        update_rx,
        Arc::clone(&pending_inputs),
    );
    let input_sink = ChannelInputSink::new(input_tx, pending_inputs);

    run_windowed_client(Box::new(client_runtime), Box::new(input_sink), HUMAN_ID, texture_manager);
}

fn run_host(options: HostLaunchOptions) {
    let port = options.port;
    thread::spawn(move || run_server(ServerLaunchOptions { port }));
    run_client(ClientLaunchOptions {
        server_ip: Some(String::from("127.0.0.1")),
        server_port: options.port,
    });
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
    let level = Arc::new(load_embedded_level());
    let server = build_headless_server(Arc::clone(&level));
    let clock_manager = ClockManager::with_server(level, server);
    run_network_server_loop(clock_manager, options.port);
}

fn run_network_server_loop(mut clock_manager: ClockManager, port: u16) {
    let listener = TcpListener::bind(("0.0.0.0", port)).unwrap_or_else(|err| {
        panic!("failed to bind tcp listener on port {port}: {err}");
    });
    listener
        .set_nonblocking(true)
        .expect("failed to set tcp listener nonblocking");

    let tick_duration = Duration::from_secs_f64(TICK_DT);
    let mut next_tick = Instant::now() + tick_duration;
    let mut next_controller_id = 1u64;

    loop {
        accept_pending_clients(&listener, clock_manager.server_mut().expect("server should exist"), &mut next_controller_id);

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

fn accept_pending_clients(listener: &TcpListener, server: &mut Server, next_controller_id: &mut u64) {
    loop {
        match listener.accept() {
            Ok((stream, _addr)) => {
                let controller_id = allocate_controller_id(server, next_controller_id);
                let controller = build_tcp_controller(stream, controller_id);
                server.add_controller(Box::new(controller), 21.0, 11.0);
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(_) => break,
        }
    }
}

fn allocate_controller_id(server: &Server, next_controller_id: &mut u64) -> u64 {
    while server.state.players.contains_key(next_controller_id) {
        *next_controller_id += 1;
    }
    let id = *next_controller_id;
    *next_controller_id += 1;
    id
}

fn build_tcp_controller(stream: TcpStream, controller_id: u64) -> ChannelController {
    let (input_tx, input_rx) = mpsc::channel();
    let (update_tx, update_rx) = mpsc::channel();
    let transport_state = Arc::new(std::sync::Mutex::new(
        net::channel_controller::ChannelTransportState::default(),
    ));

    stream
        .set_nonblocking(false)
        .expect("failed to set accepted tcp stream blocking");
    let _ = stream.set_nodelay(true);

    let read_stream = stream
        .try_clone()
        .expect("failed to clone tcp stream for server reader");
    thread::spawn({
        let transport_state = Arc::clone(&transport_state);
        move || server_read_loop(read_stream, input_tx, transport_state)
    });
    thread::spawn(move || server_write_loop(stream, update_rx));

    ChannelController::new(controller_id, input_rx, update_tx, transport_state)
}

fn server_read_loop(
    stream: TcpStream,
    input_tx: mpsc::Sender<input::InputMessage>,
    transport_state: Arc<std::sync::Mutex<net::channel_controller::ChannelTransportState>>,
) {
    let mut reader = std::io::BufReader::new(stream);
    let mut buffer = Vec::new();

    while let Ok(Some(message)) = transport::read_message::<ClientMessage>(&mut reader, &mut buffer) {
        let ClientMessage::Input(input) = message;
        {
            let mut transport_state = transport_state.lock().unwrap();
            transport_state.received_count += 1;
            transport_state.last_received_input = Some(input.clone());
        }
        if input_tx.send(input).is_err() {
            break;
        }
    }
}

fn server_write_loop(
    mut stream: TcpStream,
    update_rx: mpsc::Receiver<runtime::AuthoritativeUpdate>,
) {
    while let Ok(update) = update_rx.recv() {
        if transport::write_message(&mut stream, &ServerMessage::AuthoritativeUpdate(update)).is_err() {
            break;
        }
    }
}

fn start_tcp_client_transport(
    server_addr: String,
    input_rx: mpsc::Receiver<input::InputMessage>,
    update_tx: mpsc::Sender<runtime::AuthoritativeUpdate>,
) {
    thread::spawn(move || {
        loop {
            match TcpStream::connect(&server_addr) {
                Ok(stream) => {
                    let _ = stream.set_nodelay(true);
                    let read_stream = match stream.try_clone() {
                        Ok(stream) => stream,
                        Err(_) => return,
                    };

                    let writer = thread::spawn(move || client_write_loop(stream, input_rx));
                    client_read_loop(read_stream, update_tx);
                    let _ = writer.join();
                    return;
                }
                Err(_) => thread::sleep(Duration::from_millis(500)),
            }
        }
    });
}

fn client_write_loop(mut stream: TcpStream, input_rx: mpsc::Receiver<input::InputMessage>) {
    while let Ok(input) = input_rx.recv() {
        if transport::write_message(&mut stream, &ClientMessage::Input(input)).is_err() {
            break;
        }
    }
}

fn client_read_loop(stream: TcpStream, update_tx: mpsc::Sender<runtime::AuthoritativeUpdate>) {
    let mut reader = std::io::BufReader::new(stream);
    let mut buffer = Vec::new();

    while let Ok(Some(message)) = transport::read_message::<ServerMessage>(&mut reader, &mut buffer) {
        let ServerMessage::AuthoritativeUpdate(update) = message;
        if update_tx.send(update).is_err() {
            break;
        }
    }
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
