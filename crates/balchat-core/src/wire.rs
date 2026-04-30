//! Wire format de balchat: frames length-prefixed con payload CBOR tagged.
//!
//! Cada frame: `[u32 BE length][CBOR bytes]`.
//! El CBOR contiene un [`Frame`] enum.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Límite defensivo: 16 MB por frame. KeyPackages y mensajes individuales son <<1 MB;
/// el límite es para chunks de archivo en futuras fases.
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Versión del protocolo balchat (no MLS) en el handshake.
pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub enum Frame {
    /// Saludo balchat (antes de MLS).
    ///
    /// `my_onion`: la dirección `.onion` del que envía (auto-declarada).
    ///   El receptor NO la usa para autenticación — la autenticación viene
    ///   de Tor (al dial a una `.onion`, Tor verifica la clave del peer)
    ///   y de MLS (signature keys validadas en cada KeyPackage / message).
    ///   Es solo para que el acceptor sepa qué `.onion` registrar como
    ///   contact tras un handshake nuevo.
    ///
    /// `resume_group_id`: si el emisor tiene un MlsGroup ya guardado para
    ///   este peer, manda su group_id. Si ambos lados envían el mismo
    ///   group_id, saltamos el handshake nuevo y reanudamos.
    Hello {
        protocol_version: u16,
        my_onion: String,
        resume_group_id: Option<Vec<u8>>,
    },

    /// KeyPackage MLS serializado en TLS-encoding (output de tls_serialize_detached).
    KeyPackage(#[serde(with = "serde_bytes")] Vec<u8>),

    /// MlsMessageOut serializado: Welcome, Commit, o Application.
    MlsMessage(#[serde(with = "serde_bytes")] Vec<u8>),

    /// Cierre limpio.
    Bye,
}

pub async fn send_frame<W>(w: &mut W, frame: &Frame) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let mut buf = Vec::with_capacity(256);
    ciborium::ser::into_writer(frame, &mut buf).map_err(|e| anyhow!("CBOR encode: {e}"))?;
    if buf.len() > MAX_FRAME_BYTES {
        return Err(anyhow!("frame de {} bytes excede MAX_FRAME_BYTES", buf.len()));
    }
    let len = buf.len() as u32;
    w.write_all(&len.to_be_bytes()).await?;
    w.write_all(&buf).await?;
    w.flush().await?;
    Ok(())
}

pub async fn recv_frame<R>(r: &mut R) -> Result<Frame>
where
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
    let frame: Frame = ciborium::de::from_reader(&buf[..])
        .map_err(|e| anyhow!("CBOR decode: {e}"))?;
    Ok(frame)
}
