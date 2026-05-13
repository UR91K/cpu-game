use std::io::{self, BufReader, Read, Write};
use std::net::TcpStream;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::input::InputMessage;
use crate::runtime::AuthoritativeUpdate;

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