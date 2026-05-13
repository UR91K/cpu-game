use std::io::{self, BufRead, BufReader, Write};
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
    buffer: &mut String,
) -> io::Result<Option<T>> {
    buffer.clear();
    let bytes = reader.read_line(buffer)?;
    if bytes == 0 {
        return Ok(None);
    }

    let trimmed = buffer.trim_end_matches(['\r', '\n']);
    let message = serde_json::from_str(trimmed)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    Ok(Some(message))
}

pub fn write_message<T: Serialize>(stream: &mut TcpStream, message: &T) -> io::Result<()> {
    serde_json::to_writer(&mut *stream, message)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    stream.write_all(b"\n")?;
    stream.flush()
}