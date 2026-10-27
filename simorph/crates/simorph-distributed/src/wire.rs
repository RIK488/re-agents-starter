//! JSON-lines framing over TCP.

use serde::{de::DeserializeOwned, Serialize};
use simorph_core::{Result, SimorphError};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

/// Maximum accepted line length (protects against unbounded memory use).
pub const MAX_LINE_BYTES: usize = 64 * 1024 * 1024;

pub fn send<T: Serialize>(stream: &mut TcpStream, msg: &T) -> Result<()> {
    let mut line = serde_json::to_vec(msg)?;
    line.push(b'\n');
    stream.write_all(&line)?;
    stream.flush()?;
    Ok(())
}

/// Read one message; `Ok(None)` on clean EOF.
pub fn recv<T: DeserializeOwned>(reader: &mut BufReader<TcpStream>) -> Result<Option<T>> {
    let mut buf = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if buf.is_empty() {
                Ok(None)
            } else {
                Err(SimorphError::Io("connection closed mid-message".into()))
            };
        }
        if let Some(pos) = available.iter().position(|b| *b == b'\n') {
            buf.extend_from_slice(&available[..pos]);
            reader.consume(pos + 1);
            break;
        }
        let n = available.len();
        buf.extend_from_slice(available);
        reader.consume(n);
        if buf.len() > MAX_LINE_BYTES {
            return Err(SimorphError::Io(format!(
                "message exceeds {MAX_LINE_BYTES} bytes"
            )));
        }
    }
    if buf.iter().all(|b| b.is_ascii_whitespace()) {
        return recv(reader);
    }
    Ok(Some(serde_json::from_slice(&buf)?))
}
