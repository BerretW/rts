//! Kodek: délkový prefix (u32 BE) + JSON.

use serde::{de::DeserializeOwned, Serialize};

/// Zakóduje zprávu jako `[4 B délka big-endian][JSON bytes]`.
pub fn encode_msg<T: Serialize>(msg: &T) -> Result<Vec<u8>, serde_json::Error> {
    let json = serde_json::to_vec(msg)?;
    let len  = json.len() as u32;
    let mut out = Vec::with_capacity(4 + json.len());
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&json);
    Ok(out)
}

/// Dekóduje zprávu z JSON bytů (bez délkového prefixu).
pub fn decode_msg<T: DeserializeOwned>(json: &[u8]) -> Result<T, serde_json::Error> {
    serde_json::from_slice(json)
}

// ── Async helpers (feature = "async") ────────────────────────────────────────

#[cfg(feature = "async")]
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Zapíše zprávu do asynchronního writeru (délkový prefix + JSON).
#[cfg(feature = "async")]
pub async fn write_msg<W, T>(writer: &mut W, msg: &T) -> std::io::Result<()>
where
    W: AsyncWriteExt + Unpin,
    T: Serialize,
{
    let json = serde_json::to_vec(msg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let len = json.len() as u32;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&json).await?;
    writer.flush().await?;
    Ok(())
}

/// Přečte jednu zprávu z asynchronního readeru.
#[cfg(feature = "async")]
pub async fn read_msg<R, T>(reader: &mut R) -> std::io::Result<T>
where
    R: AsyncReadExt + Unpin,
    T: DeserializeOwned,
{
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len > crate::MAX_MSG_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("zpráva příliš velká: {len} B"),
        ));
    }

    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    serde_json::from_slice(&buf)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}
