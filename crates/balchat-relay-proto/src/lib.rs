//! Wire protocol entre cliente balchat y un balchat-relay.
//!
//! Modelo: cada conexión hace UN request → UNA response → EOF. No hay multiplexación.
//! El blob viaja como bytes opacos — el relay no sabe descifrarlo (es ciphertext MLS).

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Puerto virtual del onion service del relay.
pub const VIRTUAL_PORT: u16 = 1235;

/// Versión del protocolo relay (separada del protocol balchat principal).
pub const PROTOCOL_VERSION: u16 = 1;

/// Límite por frame (request o response). 16 MB cubre incluso archivos grandes en chunks.
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Longitud canónica del queue_id (32 bytes random, suficiente para no-colisión).
pub const QUEUE_ID_LEN: usize = 32;

#[derive(Debug, Serialize, Deserialize)]
pub enum RelayRequest {
    /// Depositar un blob de mensaje en una queue. El relay devuelve un seq monotónico.
    Put {
        protocol_version: u16,
        #[serde(with = "serde_bytes")]
        queue_id: Vec<u8>,
        #[serde(with = "serde_bytes")]
        blob: Vec<u8>,
    },
    /// Obtener mensajes de una queue con seq > since_seq, hasta `max`.
    Get {
        protocol_version: u16,
        #[serde(with = "serde_bytes")]
        queue_id: Vec<u8>,
        since_seq: u64,
        max_messages: u32,
    },
    /// Depositar un KeyPackage MLS en el pool del peer dueño de `queue_id`.
    /// Permite que A invite a B aunque B esté offline: A consume un KP del pool,
    /// genera Welcome con él, y lo publica como mensaje normal.
    PutKeyPackage {
        protocol_version: u16,
        #[serde(with = "serde_bytes")]
        queue_id: Vec<u8>,
        #[serde(with = "serde_bytes")]
        key_package: Vec<u8>,
    },
    /// Consume un KeyPackage del pool de `queue_id` (lo elimina del relay).
    /// Devuelve `None` si el pool está vacío. El peer dueño debe re-poblar.
    ConsumeKeyPackage {
        protocol_version: u16,
        #[serde(with = "serde_bytes")]
        queue_id: Vec<u8>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum RelayResponse {
    PutAck { seq: u64 },
    GetReply { messages: Vec<QueueMessage> },
    PutKeyPackageAck { pool_size: u32 },
    ConsumeKeyPackageReply {
        key_package: Option<Vec<u8>>,
    },
    Error { msg: String },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QueueMessage {
    pub seq: u64,
    #[serde(with = "serde_bytes")]
    pub blob: Vec<u8>,
}

pub async fn send_frame<T, W>(w: &mut W, value: &T) -> Result<()>
where
    T: Serialize,
    W: AsyncWrite + Unpin,
{
    let mut buf = Vec::with_capacity(256);
    ciborium::ser::into_writer(value, &mut buf).map_err(|e| anyhow!("CBOR encode: {e}"))?;
    if buf.len() > MAX_FRAME_BYTES {
        return Err(anyhow!("frame {} bytes excede MAX_FRAME_BYTES", buf.len()));
    }
    let len = buf.len() as u32;
    w.write_all(&len.to_be_bytes()).await?;
    w.write_all(&buf).await?;
    w.flush().await?;
    Ok(())
}

pub async fn recv_frame<T, R>(r: &mut R) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
    R: AsyncRead + Unpin,
{
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(anyhow!("frame anunciado {} bytes excede límite", len));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).await?;
    ciborium::de::from_reader(&buf[..]).map_err(|e| anyhow!("CBOR decode: {e}"))
}
