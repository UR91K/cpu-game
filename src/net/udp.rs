use std::collections::HashMap;
use std::io;
use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::clock::ClockManager;
use crate::input::{self, InputMessage};
use crate::net::channel_controller::{ChannelController, ChannelTransportState};
use crate::runtime::{self, AuthoritativeUpdate};
use crate::simulation::TICK_DT;

use super::ack_tracker::AckTracker;
use super::reliable::{ReliableChannel, ReliableMessage, ReliablePayload};

const MAX_PACKET_SIZE: usize = 64 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PacketHeader {
    pub sequence: u16,
    pub ack: u16,
    pub ack_bits: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClientPacket {
    pub header: PacketHeader,
    pub input: InputMessage,
    pub reliable: Vec<ReliablePayload>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerPacket {
    pub header: PacketHeader,
    pub update: AuthoritativeUpdate,
    pub reliable: Vec<ReliablePayload>,
}

pub struct ClientTransport {
    pub input_tx: mpsc::Sender<InputMessage>,
    pub update_rx: mpsc::Receiver<AuthoritativeUpdate>,
}

pub struct UdpClientSlot {
    pub addr: SocketAddr,
    pub controller_id: u64,
    pub input_tx: mpsc::Sender<InputMessage>,
    pub update_rx: mpsc::Receiver<AuthoritativeUpdate>,
    pub recv_ack_tracker: AckTracker,
    pub send_sequence: u16,
    pub reliable: ReliableChannel,
    pub transport_state: Arc<Mutex<ChannelTransportState>>,
}

struct ServerSendJob {
    addr: SocketAddr,
    packet: ServerPacket,
}

pub fn connect_client(server_addr: String) -> ClientTransport {
    let (input_tx, input_rx) = mpsc::channel();
    let (update_tx, update_rx) = mpsc::channel();
    let (reliable_tx, _reliable_rx) = mpsc::channel();
    start_udp_client_transport(server_addr, input_rx, update_tx, reliable_tx);
    ClientTransport {
        input_tx,
        update_rx,
    }
}

pub fn start_udp_client_transport(
    server_addr: String,
    input_rx: mpsc::Receiver<input::InputMessage>,
    update_tx: mpsc::Sender<runtime::AuthoritativeUpdate>,
    reliable_tx: mpsc::Sender<ReliableMessage>,
) {
    thread::spawn(move || {
        loop {
            let socket = match UdpSocket::bind("0.0.0.0:0") {
                Ok(socket) => socket,
                Err(_) => {
                    thread::sleep(Duration::from_millis(500));
                    continue;
                }
            };

            if socket.connect(&server_addr).is_err() {
                thread::sleep(Duration::from_millis(500));
                continue;
            }

            if socket.set_nonblocking(true).is_err() {
                return;
            }

            let mut send_sequence = 0u16;
            let mut recv_tracker = AckTracker::default();
            let mut reliable = ReliableChannel::default();
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let tick_duration = Duration::from_secs_f64(TICK_DT);
            let mut next_send = Instant::now();
            let mut latest_input = InputMessage::default();
            let mut pending_rotate_delta = 0.0;
            let mut pending_fire = false;
            let mut saw_fresh_input = false;

            loop {
                loop {
                    match socket.recv(&mut buf) {
                        Ok(len) => {
                            if let Ok(packet) = deserialize::<ServerPacket>(&buf[..len]) {
                                recv_tracker.record(packet.header.sequence);
                                reliable.on_ack(packet.header.ack, packet.header.ack_bits);
                                if update_tx.send(packet.update).is_err() {
                                    return;
                                }
                                for payload in packet.reliable {
                                    if reliable_tx.send(payload.message).is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                        Err(_) => break,
                    }
                }

                while let Ok(input) = input_rx.try_recv() {
                    pending_rotate_delta += input.rotate_delta;
                    pending_fire |= input.fire;
                    latest_input = merge_stateful_input(latest_input, input);
                    saw_fresh_input = true;
                }

                let now = Instant::now();
                if now >= next_send {
                    let packet_sequence = send_sequence;
                    let mut packet_input = latest_input.clone();
                    if saw_fresh_input {
                        packet_input.rotate_delta = pending_rotate_delta;
                        packet_input.fire = pending_fire;
                    } else {
                        packet_input.rotate_delta = 0.0;
                        packet_input.fire = false;
                    }
                    let packet = ClientPacket {
                        header: PacketHeader {
                            sequence: packet_sequence,
                            ack: recv_tracker.ack(),
                            ack_bits: recv_tracker.ack_bits(),
                        },
                        input: packet_input,
                        reliable: reliable.collect_for_send(packet_sequence),
                    };
                    send_sequence = send_sequence.wrapping_add(1);
                    pending_rotate_delta = 0.0;
                    pending_fire = false;
                    saw_fresh_input = false;

                    if let Ok(bytes) = serialize(&packet) {
                        if socket.send(&bytes).is_err() {
                            break;
                        }
                    }
                    next_send += tick_duration;
                }

                thread::sleep(Duration::from_millis(1));
            }

            thread::sleep(Duration::from_millis(500));
        }
    });
}

pub fn run_udp_server_loop(mut clock_manager: ClockManager, port: u16) {
    let socket = UdpSocket::bind(("0.0.0.0", port)).unwrap_or_else(|err| {
        panic!("failed to bind udp socket on port {port}: {err}");
    });
    socket
        .set_nonblocking(true)
        .expect("failed to set udp socket nonblocking");
    let send_socket = socket
        .try_clone()
        .expect("failed to clone udp socket for send loop");
    let (send_tx, send_rx) = mpsc::channel();
    thread::spawn(move || server_send_loop(send_socket, send_rx));

    let tick_duration = Duration::from_secs_f64(TICK_DT);
    let mut next_tick = Instant::now() + tick_duration;
    let mut clients: HashMap<SocketAddr, UdpClientSlot> = HashMap::new();
    let mut buf = [0u8; MAX_PACKET_SIZE];

    loop {
        loop {
            match socket.recv_from(&mut buf) {
                Ok((len, addr)) => {
                    let slot = clients.entry(addr).or_insert_with(|| {
                        create_client_slot(
                            clock_manager.server_mut().expect("server should exist"),
                            addr,
                        )
                    });

                    if let Ok(packet) = deserialize::<ClientPacket>(&buf[..len]) {
                        slot.recv_ack_tracker.record(packet.header.sequence);
                        slot.reliable
                            .on_ack(packet.header.ack, packet.header.ack_bits);
                        {
                            let mut transport_state = slot.transport_state.lock().unwrap();
                            transport_state.received_count += 1;
                            transport_state.last_received_input = Some(packet.input.clone());
                        }
                        if slot.input_tx.send(packet.input).is_err() {
                            continue;
                        }
                        for payload in packet.reliable {
                            handle_reliable_client_message(slot.controller_id, payload.message);
                        }
                    }
                }
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }

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

        for slot in clients.values_mut() {
            let mut latest_update = None;
            while let Ok(update) = slot.update_rx.try_recv() {
                latest_update = Some(update);
            }

            let Some(update) = latest_update else {
                continue;
            };

            let packet_sequence = slot.send_sequence;
            let packet = ServerPacket {
                header: PacketHeader {
                    sequence: packet_sequence,
                    ack: slot.recv_ack_tracker.ack(),
                    ack_bits: slot.recv_ack_tracker.ack_bits(),
                },
                update,
                reliable: slot.reliable.collect_for_send(packet_sequence),
            };
            slot.send_sequence = slot.send_sequence.wrapping_add(1);

            let _ = send_tx.send(ServerSendJob {
                addr: slot.addr,
                packet,
            });
        }
    }
}

fn create_client_slot(server: &mut super::server::Server, addr: SocketAddr) -> UdpClientSlot {
    let controller_id = server.allocate_controller_id();
    let (input_tx, input_rx) = mpsc::channel();
    let (update_tx, update_rx) = mpsc::channel();
    let transport_state = Arc::new(Mutex::new(ChannelTransportState::default()));
    let controller = ChannelController::new(
        controller_id,
        input_rx,
        update_tx,
        Arc::clone(&transport_state),
    );
    server.add_controller(Box::new(controller), 21.0, 11.0);

    UdpClientSlot {
        addr,
        controller_id,
        input_tx,
        update_rx,
        recv_ack_tracker: AckTracker::default(),
        send_sequence: 0,
        reliable: ReliableChannel::default(),
        transport_state,
    }
}

fn handle_reliable_client_message(_controller_id: u64, _message: ReliableMessage) {}

fn server_send_loop(socket: UdpSocket, send_rx: mpsc::Receiver<ServerSendJob>) {
    while let Ok(job) = send_rx.recv() {
        if let Ok(bytes) = serialize(&job.packet) {
            let _ = socket.send_to(&bytes, job.addr);
        }
    }
}

fn merge_stateful_input(mut current: InputMessage, latest: InputMessage) -> InputMessage {
    current.controller_id = latest.controller_id;
    current.tick = latest.tick;
    current.forward = latest.forward;
    current.back = latest.back;
    current.strafe_left = latest.strafe_left;
    current.strafe_right = latest.strafe_right;
    current
}

fn serialize<T: Serialize>(message: &T) -> io::Result<Vec<u8>> {
    bincode::serialize(message).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

fn deserialize<T: DeserializeOwned>(bytes: &[u8]) -> io::Result<T> {
    bincode::deserialize(bytes).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}
