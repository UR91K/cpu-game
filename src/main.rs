use std::env;
use std::process::Command;
use std::sync::Arc;

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
mod server_runner;
mod simulation;
mod text_layer;
mod texture;

use app::App;
use clock::ClockManager;
use input::{ChannelInputSink, InputSink};
use level::load_embedded_level;
use runtime::{ChannelClientRuntime, GameRuntime};

use crate::net::server::build_headless_server;
use crate::server_runner::run_network_server_loop;

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
    let transport = net::udp::connect_client(requested_server_addr);
    let pending_inputs = Arc::new(std::sync::Mutex::new(Vec::new()));
    let client_runtime = ChannelClientRuntime::new(
        Arc::clone(&level),
        transport.update_rx,
        Arc::clone(&pending_inputs),
    );
    let input_sink = ChannelInputSink::new(transport.input_tx, pending_inputs);

    run_windowed_client(
        Box::new(client_runtime),
        Box::new(input_sink),
        HUMAN_ID,
        texture_manager,
    );
}

fn run_host(options: HostLaunchOptions) {
    let current_exe = env::current_exe().unwrap_or_else(|err| {
        panic!("failed to resolve current executable for host mode: {err}");
    });
    let mut server_process = Command::new(current_exe)
        .arg("server")
        .arg("--port")
        .arg(options.port.to_string())
        .spawn()
        .unwrap_or_else(|err| panic!("failed to spawn server process for host mode: {err}"));

    run_client(ClientLaunchOptions {
        server_ip: Some(String::from("127.0.0.1")),
        server_port: options.port,
    });

    let _ = server_process.kill();
    let _ = server_process.wait();
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
