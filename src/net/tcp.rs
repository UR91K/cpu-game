use std::io::{self, BufReader, Read, Write};
use std::thread;

use std::net::TcpStream;

use std::sync::Arc;
use std::sync::mpsc;

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::input::{self, InputMessage};
use crate::net::channel_controller::{self, ChannelController};
use crate::net::{self, tcp};
use crate::runtime::{self, AuthoritativeUpdate};

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    Input(InputMessage),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ServerMessage {
    AuthoritativeUpdate(AuthoritativeUpdate),
}

pub fn read_message<T: DeserializeOwned>(
    reader: &mut BufReader<TcpStream>,
    buffer: &mut Vec<u8>,
) -> io::Result<Option<T>> {
    buffer.clear();
    let mut len_bytes = [0u8; 4];
    match reader.read_exact(&mut len_bytes) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(err) => return Err(err),
    }
    let message_len = u32::from_le_bytes(len_bytes) as usize;
    buffer.resize(message_len, 0);
    reader.read_exact(buffer)?;

    let message = bincode::deserialize(buffer)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    Ok(Some(message))
}

pub fn write_message<T: Serialize>(stream: &mut TcpStream, message: &T) -> io::Result<()> {
    let encoded = bincode::serialize(message)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let len = u32::try_from(encoded.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "message too large"))?;
    stream.write_all(&len.to_le_bytes())?;
    stream.write_all(&encoded)
}

pub fn build_tcp_controller(stream: TcpStream, controller_id: u64) -> ChannelController {
    let (input_tx, input_rx) = mpsc::channel();
    let (update_tx, update_rx) = mpsc::channel();
    let transport_state = Arc::new(std::sync::Mutex::new(
        channel_controller::ChannelTransportState::default(),
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

pub fn server_read_loop(
    stream: TcpStream,
    input_tx: mpsc::Sender<input::InputMessage>,
    transport_state: Arc<std::sync::Mutex<net::channel_controller::ChannelTransportState>>,
) {
    let mut reader = std::io::BufReader::new(stream);
    let mut buffer = Vec::new();

    while let Ok(Some(message)) = tcp::read_message::<ClientMessage>(&mut reader, &mut buffer) {
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

pub fn server_write_loop(
    mut stream: TcpStream,
    update_rx: mpsc::Receiver<runtime::AuthoritativeUpdate>,
) {
    while let Ok(update) = update_rx.recv() {
        if tcp::write_message(&mut stream, &ServerMessage::AuthoritativeUpdate(update)).is_err() {
            break;
        }
    }
}

pub fn start_tcp_client_transport(
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

pub fn client_write_loop(mut stream: TcpStream, input_rx: mpsc::Receiver<input::InputMessage>) {
    while let Ok(input) = input_rx.recv() {
        if tcp::write_message(&mut stream, &ClientMessage::Input(input)).is_err() {
            break;
        }
    }
}

pub fn client_read_loop(stream: TcpStream, update_tx: mpsc::Sender<runtime::AuthoritativeUpdate>) {
    let mut reader = std::io::BufReader::new(stream);
    let mut buffer = Vec::new();

    while let Ok(Some(message)) = tcp::read_message::<ServerMessage>(&mut reader, &mut buffer) {
        let ServerMessage::AuthoritativeUpdate(update) = message;
        if update_tx.send(update).is_err() {
            break;
        }
    }
}
