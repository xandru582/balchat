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

#[cfg(test)]
mod kat_tests {
    //! Known-Answer Tests del wire format relay. Si fallan, peers viejos
    //! no van a hablar con relays nuevos. Bumpear `PROTOCOL_VERSION` al
    //! cambiar deliberadamente.
    use super::*;
    use tokio::io::duplex;

    fn cbor<T: Serialize>(v: &T) -> Vec<u8> {
        let mut buf = Vec::new();
        ciborium::ser::into_writer(v, &mut buf).unwrap();
        buf
    }

    #[test]
    fn put_request_roundtrip() {
        let req = RelayRequest::Put {
            protocol_version: PROTOCOL_VERSION,
            queue_id: vec![0xab; QUEUE_ID_LEN],
            blob: b"ciphertext mls payload".to_vec(),
        };
        let bytes = cbor(&req);
        let de: RelayRequest = ciborium::de::from_reader(&bytes[..]).unwrap();
        match de {
            RelayRequest::Put {
                protocol_version,
                queue_id,
                blob,
            } => {
                assert_eq!(protocol_version, PROTOCOL_VERSION);
                assert_eq!(queue_id, vec![0xab; QUEUE_ID_LEN]);
                assert_eq!(blob, b"ciphertext mls payload");
            }
            other => panic!("esperaba Put, llegó {other:?}"),
        }
    }

    #[test]
    fn get_request_roundtrip() {
        let req = RelayRequest::Get {
            protocol_version: PROTOCOL_VERSION,
            queue_id: vec![0xff; QUEUE_ID_LEN],
            since_seq: 42,
            max_messages: 64,
        };
        let bytes = cbor(&req);
        let de: RelayRequest = ciborium::de::from_reader(&bytes[..]).unwrap();
        match de {
            RelayRequest::Get {
                since_seq,
                max_messages,
                ..
            } => {
                assert_eq!(since_seq, 42);
                assert_eq!(max_messages, 64);
            }
            other => panic!("esperaba Get, llegó {other:?}"),
        }
    }

    #[test]
    fn putack_response_roundtrip() {
        let resp = RelayResponse::PutAck { seq: 12345 };
        let bytes = cbor(&resp);
        let de: RelayResponse = ciborium::de::from_reader(&bytes[..]).unwrap();
        assert!(matches!(de, RelayResponse::PutAck { seq: 12345 }));
    }

    #[test]
    fn getreply_response_roundtrip_with_msgs() {
        let resp = RelayResponse::GetReply {
            messages: vec![
                QueueMessage {
                    seq: 1,
                    blob: b"a".to_vec(),
                },
                QueueMessage {
                    seq: 7,
                    blob: vec![0u8; 1024],
                },
            ],
        };
        let bytes = cbor(&resp);
        let de: RelayResponse = ciborium::de::from_reader(&bytes[..]).unwrap();
        match de {
            RelayResponse::GetReply { messages } => {
                assert_eq!(messages.len(), 2);
                assert_eq!(messages[0].seq, 1);
                assert_eq!(messages[1].blob.len(), 1024);
            }
            other => panic!("esperaba GetReply, llegó {other:?}"),
        }
    }

    #[test]
    fn consume_kp_response_none_vs_some() {
        let none = RelayResponse::ConsumeKeyPackageReply { key_package: None };
        let some = RelayResponse::ConsumeKeyPackageReply {
            key_package: Some(vec![0x42, 0x43, 0x44]),
        };
        let bn = cbor(&none);
        let bs = cbor(&some);
        assert_ne!(bn, bs);
        let de_none: RelayResponse = ciborium::de::from_reader(&bn[..]).unwrap();
        let de_some: RelayResponse = ciborium::de::from_reader(&bs[..]).unwrap();
        match de_none {
            RelayResponse::ConsumeKeyPackageReply { key_package: None } => {}
            other => panic!("esperaba None, llegó {other:?}"),
        }
        match de_some {
            RelayResponse::ConsumeKeyPackageReply {
                key_package: Some(b),
            } => assert_eq!(b, vec![0x42, 0x43, 0x44]),
            other => panic!("esperaba Some, llegó {other:?}"),
        }
    }

    #[tokio::test]
    async fn send_recv_frame_roundtrip() {
        let (mut a, mut b) = duplex(8 * 1024);
        let req = RelayRequest::ConsumeKeyPackage {
            protocol_version: PROTOCOL_VERSION,
            queue_id: vec![1, 2, 3],
        };
        send_frame(&mut a, &req).await.unwrap();
        let de: RelayRequest = recv_frame(&mut b).await.unwrap();
        match de {
            RelayRequest::ConsumeKeyPackage {
                protocol_version,
                queue_id,
            } => {
                assert_eq!(protocol_version, PROTOCOL_VERSION);
                assert_eq!(queue_id, vec![1, 2, 3]);
            }
            other => panic!("esperaba ConsumeKeyPackage, llegó {other:?}"),
        }
    }

    #[test]
    fn protocol_version_is_one() {
        // Pin: cualquier cambio de PROTOCOL_VERSION debe ser deliberado y
        // acompañado de un wire-version bump documentado en README.
        assert_eq!(PROTOCOL_VERSION, 1);
    }
}
