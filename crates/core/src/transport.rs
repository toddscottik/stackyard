use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::{Error, Result};
use crate::protocol::MAX_MESSAGE_BYTES;

pub async fn send<T, W>(writer: &mut W, value: &T) -> Result<()>
where
    T: Serialize,
    W: AsyncWrite + Unpin,
{
    let mut bytes = serde_json::to_vec(value)
        .map_err(|err| Error::InvalidMessage(format!("encode message: {err}")))?;
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(Error::InvalidMessage(format!(
            "message is {} bytes; the limit is {MAX_MESSAGE_BYTES}",
            bytes.len()
        )));
    }
    bytes.push(b'\n');
    writer
        .write_all(&bytes)
        .await
        .map_err(|source| Error::io("write engine message", source))?;
    Ok(())
}

pub async fn recv<T, R>(reader: &mut R) -> Result<T>
where
    T: DeserializeOwned,
    R: AsyncBufRead + Unpin,
{
    let mut buf = Vec::new();
    loop {
        if buf.len() > MAX_MESSAGE_BYTES {
            return Err(Error::InvalidMessage(format!(
                "message is longer than {MAX_MESSAGE_BYTES} bytes"
            )));
        }
        let available = reader
            .fill_buf()
            .await
            .map_err(|source| Error::io("read engine message", source))?;
        if available.is_empty() {
            break;
        }
        if let Some(pos) = available.iter().position(|byte| *byte == b'\n') {
            buf.extend_from_slice(&available[..=pos]);
            let consumed = pos + 1;
            reader.consume(consumed);
            break;
        }
        let n = available.len();
        buf.extend_from_slice(available);
        reader.consume(n);
    }
    if buf.is_empty() {
        return Err(Error::InvalidMessage(
            "engine closed the connection before a reply".to_string(),
        ));
    }
    if buf.last() == Some(&b'\n') {
        buf.pop();
    }
    serde_json::from_slice(&buf)
        .map_err(|err| Error::InvalidMessage(format!("decode message: {err}")))
}
