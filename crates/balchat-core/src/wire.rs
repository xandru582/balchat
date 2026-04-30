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

#[cfg(test)]
mod kat_tests {
    //! Known-Answer Tests del wire format. Si cualquiera de estos falla,
    //! el wire format cambió de manera incompatible — peers viejos no van
    //! a poder hablar con peers nuevos. Bumpear `PROTOCOL_VERSION` y
    //! actualizar los vectores deliberadamente cuando esto suceda.
    use super::*;
    use tokio::io::duplex;

    fn cbor(frame: &Frame) -> Vec<u8> {
        let mut buf = Vec::new();
        ciborium::ser::into_writer(frame, &mut buf).unwrap();
        buf
    }

    #[test]
    fn bye_is_canonical_4_bytes() {
        // ciborium externally-tagged: unit variant `Bye` se serializa como la
        // string CBOR "Bye": 0x63 (text-string-of-length-3) + "Bye" UTF-8.
        let bytes = cbor(&Frame::Bye);
        assert_eq!(
            bytes,
            [0x63, b'B', b'y', b'e'],
            "Frame::Bye debe serializar a 4 bytes exactos: text-string('Bye')"
        );
    }

    #[test]
    fn hello_no_resume_canonical() {
        // Hello { protocol_version: 1, my_onion: "x.onion:1234", resume_group_id: None }
        // ciborium externally-tagged: { "Hello": { "protocol_version":1, ... } }
        let f = Frame::Hello {
            protocol_version: 1,
            my_onion: "x.onion:1234".into(),
            resume_group_id: None,
        };
        let bytes = cbor(&f);
        // Sanidad: que termine con "x.onion:1234" string (text-string CBOR).
        let end = b"\x6cx.onion:1234"; // 0x6c = text-string len 12
        assert!(
            bytes.windows(end.len()).any(|w| w == end),
            "Hello debería contener text-string('x.onion:1234'); got {:?}",
            hex::encode(&bytes)
        );
        // Roundtrip:
        let de: Frame = ciborium::de::from_reader(&bytes[..]).unwrap();
        match de {
            Frame::Hello {
                protocol_version,
                my_onion,
                resume_group_id,
            } => {
                assert_eq!(protocol_version, 1);
                assert_eq!(my_onion, "x.onion:1234");
                assert_eq!(resume_group_id, None);
            }
            other => panic!("esperaba Hello, llegó {other:?}"),
        }
    }

    #[tokio::test]
    async fn send_recv_frame_roundtrip_with_length_prefix() {
        // Verifica el envoltorio length-prefixed: send_frame escribe
        // `[u32 BE len][cbor]` y recv_frame lo recupera idéntico.
        let (mut a, mut b) = duplex(64 * 1024);
        let f = Frame::Hello {
            protocol_version: PROTOCOL_VERSION,
            my_onion: "alice.onion:1234".into(),
            resume_group_id: Some(vec![1, 2, 3, 4, 5]),
        };
        send_frame(&mut a, &f).await.unwrap();
        let de = recv_frame(&mut b).await.unwrap();
        match de {
            Frame::Hello {
                protocol_version,
                my_onion,
                resume_group_id,
            } => {
                assert_eq!(protocol_version, PROTOCOL_VERSION);
                assert_eq!(my_onion, "alice.onion:1234");
                assert_eq!(resume_group_id, Some(vec![1, 2, 3, 4, 5]));
            }
            other => panic!("esperaba Hello, llegó {other:?}"),
        }
    }

    #[test]
    fn keypackage_bytes_preserved() {
        let payload: Vec<u8> = (0..256u32).map(|i| (i % 256) as u8).collect();
        let f = Frame::KeyPackage(payload.clone());
        let bytes = cbor(&f);
        let de: Frame = ciborium::de::from_reader(&bytes[..]).unwrap();
        match de {
            Frame::KeyPackage(p) => assert_eq!(p, payload),
            other => panic!("esperaba KeyPackage, llegó {other:?}"),
        }
    }

    #[test]
    fn frame_max_size_enforced() {
        // Construimos un Frame::KeyPackage que (con overhead CBOR) supera
        // MAX_FRAME_BYTES. Esperamos que send_frame falle con error claro
        // sin escribir nada al stream.
        let payload = vec![0u8; MAX_FRAME_BYTES + 1024];
        let f = Frame::KeyPackage(payload);
        let mut buf: Vec<u8> = Vec::new();
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let res = rt.block_on(async { send_frame(&mut buf, &f).await });
        assert!(res.is_err(), "frame demasiado grande debe fallar");
    }
}
