//! Conversation 1:1 sobre Tor con MLS.
//!
//! Soporta dos modos:
//!   * **Handshake nuevo**: primer encuentro con un peer. Crea un grupo MLS y manda Welcome.
//!   * **Resume**: ambos lados tienen un MlsGroup persistido con el mismo `group_id`.
//!     Saltan el handshake y siguen con Application messages encima del ratchet existente.
//!
//! El identificador de la sesión MLS es el `group_id` (32 bytes random). Solo los miembros
//! del grupo lo conocen; un peer que no esté en el grupo no puede afirmar el mismo
//! group_id. Tor previene MitM.

use anyhow::{anyhow, Context, Result};
use arti_client::DataStream;
use openmls::prelude::tls_codec::Serialize as _;
use openmls::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};

use crate::identity::Identity;
use crate::wire::{recv_frame, send_frame, Frame, PROTOCOL_VERSION};

/// Payload de aplicación que viaja DENTRO de cada MLS application message.
/// Permite extender el protocolo (texto, archivo, llamadas, ...) sin cambiar wire.
#[derive(Debug, Serialize, Deserialize)]
pub enum AppPayload {
    /// Mensaje de texto UTF-8.
    Text(String),
    /// Archivo entero in-line (límite real ~14 MiB tras overhead MLS).
    /// Para archivos más grandes, ver [`AppPayload::FileChunk`].
    File {
        filename: String,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    },
    /// Un chunk de un archivo grande. Se identifican por `file_id` (16 bytes
    /// random generados por el sender); cuando el receiver tiene todos los
    /// `total_chunks`, los concatena en orden de `chunk_idx` y obtiene el
    /// archivo final. La metadata (filename / total_chunks / total_bytes) se
    /// repite en cada chunk para que el orden de llegada no importe.
    FileChunk {
        #[serde(with = "serde_bytes")]
        file_id: Vec<u8>,
        filename: String,
        total_chunks: u32,
        chunk_idx: u32,
        total_bytes: u64,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    },
}

impl AppPayload {
    pub fn text(s: impl Into<String>) -> Self {
        AppPayload::Text(s.into())
    }
}

/// Tamaño de chunk en bytes (8 MiB). MLS Application messages tienen overhead
/// (~few KiB) sobre un PrivateMessage; con 8 MiB de payload neto cabemos cómodos
/// dentro del ~14 MiB efectivo del frame.
pub const FILE_CHUNK_SIZE: usize = 8 * 1024 * 1024;

/// Threshold a partir del cual conviene cortar en chunks. Por debajo de esto
/// usamos `AppPayload::File` (un solo mensaje) porque es más eficiente.
pub const FILE_INLINE_THRESHOLD: usize = 12 * 1024 * 1024;

/// Genera un `file_id` aleatorio (16 bytes) y parte `data` en chunks de
/// `FILE_CHUNK_SIZE` bytes. Cada chunk lleva la misma metadata para que el
/// receiver pueda reensamblar incluso si los chunks llegan desordenados.
pub fn split_file_into_chunks(filename: &str, data: &[u8]) -> Vec<AppPayload> {
    use rand::RngCore;
    let mut file_id = vec![0u8; 16];
    rand::thread_rng().fill_bytes(&mut file_id);
    let total_bytes = data.len() as u64;
    let total_chunks =
        ((data.len() + FILE_CHUNK_SIZE - 1) / FILE_CHUNK_SIZE).max(1) as u32;

    data.chunks(FILE_CHUNK_SIZE)
        .enumerate()
        .map(|(i, slice)| AppPayload::FileChunk {
            file_id: file_id.clone(),
            filename: filename.to_string(),
            total_chunks,
            chunk_idx: i as u32,
            total_bytes,
            data: slice.to_vec(),
        })
        .collect()
}

/// Estado del reensamblaje de un archivo grande. Devuelto por
/// [`record_file_chunk`] cuando aún faltan chunks.
#[derive(Debug, Clone)]
pub struct ChunkProgress {
    pub filename: String,
    pub received: u32,
    pub total: u32,
    pub total_bytes: u64,
}

/// Resultado de procesar un `FileChunk`. O bien queda incompleto y devolvemos
/// progreso (`Pending`), o se completó y entregamos el archivo ensamblado
/// (`Complete`). El caller decide cómo guardarlo.
#[derive(Debug)]
pub enum ChunkOutcome {
    Pending(ChunkProgress),
    Complete {
        filename: String,
        data: Vec<u8>,
    },
}

/// Persiste el chunk en `spool_dir` (si todavía no existe), y retorna `Complete`
/// cuando ya tenemos todos los chunks (concatenados en orden + metadata
/// validada). Idempotente: re-recibir el mismo chunk no rompe nada.
///
/// Layout en disco:
/// ```text
/// <spool_dir>/<file_id_hex>/meta.cbor          -> { filename, total_chunks, total_bytes }
/// <spool_dir>/<file_id_hex>/chunk-<idx>.bin    -> bytes del chunk
/// ```
///
/// Una vez completo, el caller debe borrar el directorio parcial — esta función
/// no lo hace para evitar borrar antes de que el caller persista el archivo.
pub fn record_file_chunk(
    spool_dir: &std::path::Path,
    file_id: &[u8],
    filename: &str,
    chunk_idx: u32,
    total_chunks: u32,
    total_bytes: u64,
    chunk_data: &[u8],
) -> Result<ChunkOutcome> {
    if chunk_idx >= total_chunks {
        return Err(anyhow!(
            "chunk_idx {chunk_idx} fuera de rango (total={total_chunks})"
        ));
    }
    let id_hex: String = file_id.iter().map(|b| format!("{b:02x}")).collect();
    let dir = spool_dir.join(&id_hex);
    std::fs::create_dir_all(&dir).with_context(|| format!("crear {}", dir.display()))?;

    // Persistir meta una sola vez; si ya existe, validamos consistencia.
    let meta_path = dir.join("meta.cbor");
    if !meta_path.exists() {
        let meta = ChunkMeta {
            filename: filename.to_string(),
            total_chunks,
            total_bytes,
        };
        let mut buf = Vec::with_capacity(64);
        ciborium::ser::into_writer(&meta, &mut buf)
            .map_err(|e| anyhow!("CBOR meta: {e}"))?;
        std::fs::write(&meta_path, &buf).with_context(|| format!("escribir {}", meta_path.display()))?;
    } else {
        // Validar que no es un atacante reusando file_id con filename distinto.
        let bytes = std::fs::read(&meta_path)?;
        let m: ChunkMeta = ciborium::de::from_reader(&bytes[..])
            .map_err(|e| anyhow!("leer meta CBOR: {e}"))?;
        if m.filename != filename || m.total_chunks != total_chunks || m.total_bytes != total_bytes {
            return Err(anyhow!(
                "metadata de chunk inconsistente con la ya recibida — file_id reusado con datos distintos"
            ));
        }
    }

    let chunk_path = dir.join(format!("chunk-{chunk_idx:08}.bin"));
    if !chunk_path.exists() {
        std::fs::write(&chunk_path, chunk_data)
            .with_context(|| format!("escribir {}", chunk_path.display()))?;
    }

    let mut received: u32 = 0;
    for i in 0..total_chunks {
        if dir.join(format!("chunk-{i:08}.bin")).exists() {
            received += 1;
        }
    }
    if received < total_chunks {
        return Ok(ChunkOutcome::Pending(ChunkProgress {
            filename: filename.to_string(),
            received,
            total: total_chunks,
            total_bytes,
        }));
    }

    // Concat en orden. Nota: en archivos muy grandes esto carga todo a RAM —
    // para optimizar más tarde podemos streamear directo al destino.
    let mut data = Vec::with_capacity(total_bytes as usize);
    for i in 0..total_chunks {
        let p = dir.join(format!("chunk-{i:08}.bin"));
        let bytes = std::fs::read(&p).with_context(|| format!("leer {}", p.display()))?;
        data.extend_from_slice(&bytes);
    }
    if data.len() as u64 != total_bytes {
        return Err(anyhow!(
            "tamaño concatenado {} != total_bytes declarado {}",
            data.len(),
            total_bytes
        ));
    }

    Ok(ChunkOutcome::Complete {
        filename: filename.to_string(),
        data,
    })
}

/// Borra el directorio parcial de un file_id ya ensamblado. Idempotente.
pub fn cleanup_chunk_spool(spool_dir: &std::path::Path, file_id: &[u8]) {
    let id_hex: String = file_id.iter().map(|b| format!("{b:02x}")).collect();
    let dir = spool_dir.join(&id_hex);
    if dir.exists() {
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[derive(Serialize, Deserialize)]
struct ChunkMeta {
    filename: String,
    total_chunks: u32,
    total_bytes: u64,
}

/// Indica quién inició la conexión TCP. No afecta a MLS — solo determina el orden
/// de envío de Hello / KeyPackage / Welcome durante un handshake nuevo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Initiator,
    Acceptor,
}

/// Resultado del intercambio de Hello: o reanudamos un grupo, o hacemos uno nuevo.
#[derive(Debug)]
pub enum HandshakeOutcome {
    /// Ambos lados tienen un MlsGroup con este group_id ya en storage.
    Resumed { group_id: Vec<u8>, peer_onion: String },
    /// Hicimos handshake nuevo y MlsGroup quedó creado en este proceso.
    Fresh { group_id: Vec<u8>, peer_onion: String },
}

pub struct Conversation<S = DataStream> {
    pub group: MlsGroup,
    pub peer_onion: String,
    stream: S,
}

impl<S> Conversation<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    /// Abre una Conversation siguiendo el protocolo balchat.
    ///
    /// * `our_onion`  — nuestra `.onion` (la informamos en Hello).
    /// * `peer_onion_known` — si ya sabíamos a qué peer estamos conectando antes del Hello
    ///                       (típicamente el initiator que dial-eó), pasarlo permite que
    ///                       construyamos `resume_group_id` desde el resolver antes de
    ///                       enviar Hello. El acceptor no lo conoce aún.
    /// * `resolver`   — dado un onion address de peer, devuelve el `mls_group_id` que
    ///                 tenemos guardado para él (o `None`).
    pub async fn open(
        mut stream: S,
        identity: &Identity,
        role: Role,
        our_onion: &str,
        peer_onion_known: Option<&str>,
        resolver: &dyn ResumeResolver,
    ) -> Result<(Self, HandshakeOutcome)> {
        // Initiator: ya conoce al peer; resolver puede dar group_id desde el inicio.
        let initiator_resume = match (role, peer_onion_known) {
            (Role::Initiator, Some(p)) => resolver.group_id_for(p),
            _ => None,
        };

        // Initiator envía primero. Acceptor recibe primero, decide resume con el
        // peer.my_onion recién aprendido, y responde.
        let peer_hello = match role {
            Role::Initiator => {
                let our_hello = Frame::Hello {
                    protocol_version: PROTOCOL_VERSION,
                    my_onion: our_onion.to_string(),
                    resume_group_id: initiator_resume.clone(),
                };
                send_frame(&mut stream, &our_hello).await?;
                recv_frame(&mut stream).await?
            }
            Role::Acceptor => recv_frame(&mut stream).await?,
        };

        let (peer_onion, peer_resume) = match peer_hello {
            Frame::Hello {
                protocol_version,
                my_onion,
                resume_group_id,
            } => {
                if protocol_version != PROTOCOL_VERSION {
                    return Err(anyhow!(
                        "protocolo del peer {} != {PROTOCOL_VERSION}",
                        protocol_version
                    ));
                }
                (my_onion, resume_group_id)
            }
            other => return Err(anyhow!("se esperaba Hello, llegó {other:?}")),
        };

        // Acceptor: ahora que conoce peer_onion, decide su resume.
        // Prioridad: si peer mandó un group_id que YO conozco (puede ser 1:1 o
        // grupo n-way), uso ese; si no, fallback al group_id 1:1 con peer_onion.
        let our_resume = match role {
            Role::Initiator => initiator_resume,
            Role::Acceptor => {
                let r = if let Some(peer_gid) = peer_resume.as_deref() {
                    if resolver.knows_group_id(peer_gid) {
                        Some(peer_gid.to_vec())
                    } else {
                        resolver.group_id_for(&peer_onion)
                    }
                } else {
                    resolver.group_id_for(&peer_onion)
                };
                let our_hello = Frame::Hello {
                    protocol_version: PROTOCOL_VERSION,
                    my_onion: our_onion.to_string(),
                    resume_group_id: r.clone(),
                };
                send_frame(&mut stream, &our_hello).await?;
                r
            }
        };

        let can_resume = match (&our_resume, &peer_resume) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        };

        // Defensa contra suplantación de `.onion`:
        //
        // Si NOSOTROS tenemos un grupo MLS con este peer y el peer afirma NO tenerlo,
        // alguien con la clave privada del onion del peer podría estar tratando de
        // crear un grupo nuevo y sobrescribir el ratchet legítimo. Rechazamos.
        //
        // El caso opuesto (peer tiene grupo y nosotros no) es legítimo: probablemente
        // perdimos state local; aceptamos handshake fresh.
        if our_resume.is_some() && peer_resume.is_none() {
            return Err(anyhow!(
                "{role:?}: peer {peer_onion} dice no tener grupo MLS pero nosotros sí — \
                 posible suplantación, abortando handshake fresh para no sobreescribir state existente"
            ));
        }

        if can_resume {
            let group_id = our_resume.expect("can_resume implica Some");
            let gid = GroupId::from_slice(&group_id);
            let group = MlsGroup::load(identity.provider.storage(), &gid)
                .map_err(|e| anyhow!("MlsGroup::load: {e:?}"))?
                .ok_or_else(|| anyhow!("group_id resume reportado pero no en storage local"))?;
            tracing::info!(
                epoch = group.epoch().as_u64(),
                "{role:?}: resume MlsGroup con peer {peer_onion}"
            );
            let outcome = HandshakeOutcome::Resumed {
                group_id: group_id.clone(),
                peer_onion: peer_onion.clone(),
            };
            return Ok((
                Self {
                    group,
                    peer_onion,
                    stream,
                },
                outcome,
            ));
        }

        tracing::info!("{role:?}: handshake nuevo con peer {peer_onion}");
        let expected_pubkey = match role {
            Role::Initiator => resolver.expected_pubkey_for(&peer_onion),
            Role::Acceptor => None, // acceptor no conoce signing key del peer hasta epoch>0
        };
        let group = match role {
            Role::Initiator => {
                fresh_handshake_initiator(&mut stream, identity, expected_pubkey.as_deref()).await?
            }
            Role::Acceptor => fresh_handshake_acceptor(&mut stream, identity).await?,
        };
        let group_id = group.group_id().as_slice().to_vec();
        let outcome = HandshakeOutcome::Fresh {
            group_id,
            peer_onion: peer_onion.clone(),
        };
        Ok((
            Self {
                group,
                peer_onion,
                stream,
            },
            outcome,
        ))
    }

    /// Cifra y envía un AppPayload arbitrario.
    pub async fn send_app(&mut self, identity: &Identity, payload: &AppPayload) -> Result<()> {
        let mut buf = Vec::with_capacity(256);
        ciborium::ser::into_writer(payload, &mut buf)
            .map_err(|e| anyhow!("serializar AppPayload: {e}"))?;
        let mls_out = self
            .group
            .create_message(&identity.provider, &identity.signer, &buf)
            .context("create_message")?;
        let bytes = mls_out
            .tls_serialize_detached()
            .context("serializar mensaje MLS")?;
        send_frame(&mut self.stream, &Frame::MlsMessage(bytes)).await
    }

    /// Lee y descifra el siguiente AppPayload del peer. `Ok(None)` si peer cerró.
    pub async fn recv_app(&mut self, identity: &Identity) -> Result<Option<AppPayload>> {
        loop {
            let frame = match recv_frame(&mut self.stream).await {
                Ok(f) => f,
                Err(e) => {
                    if let Some(io) = e.downcast_ref::<std::io::Error>() {
                        if matches!(io.kind(), std::io::ErrorKind::UnexpectedEof) {
                            return Ok(None);
                        }
                    }
                    return Err(e);
                }
            };
            match frame {
                Frame::MlsMessage(bytes) => {
                    let in_msg = MlsMessageIn::tls_deserialize_exact_bytes(&bytes)
                        .context("deserializar MlsMessageIn")?;
                    let proto: ProtocolMessage = in_msg
                        .try_into_protocol_message()
                        .map_err(|_| anyhow!("frame MLS no es ProtocolMessage"))?;
                    let processed = self
                        .group
                        .process_message(&identity.provider, proto)
                        .context("process_message")?;
                    match processed.into_content() {
                        ProcessedMessageContent::ApplicationMessage(app) => {
                            let bytes = app.into_bytes();
                            // Compatibilidad: si los bytes son válidos UTF-8 pero no
                            // CBOR, los tratamos como texto plano (mensajes pre-2c).
                            let payload = match ciborium::de::from_reader::<AppPayload, _>(&bytes[..]) {
                                Ok(p) => p,
                                Err(_) => AppPayload::Text(
                                    String::from_utf8_lossy(&bytes).into_owned(),
                                ),
                            };
                            return Ok(Some(payload));
                        }
                        ProcessedMessageContent::StagedCommitMessage(staged) => {
                            // Cambio de membresía del grupo (alguien fue añadido o removido).
                            self.group
                                .merge_staged_commit(&identity.provider, *staged)
                                .context("merge_staged_commit")?;
                            tracing::info!(
                                epoch = self.group.epoch().as_u64(),
                                "commit aplicado: epoch ahora {}",
                                self.group.epoch().as_u64()
                            );
                            continue;
                        }
                        ProcessedMessageContent::ProposalMessage(_)
                        | ProcessedMessageContent::ExternalJoinProposalMessage(_) => {
                            tracing::debug!("propuesta MLS recibida, ignorando en 1:1 v1");
                            continue;
                        }
                    }
                }
                Frame::Bye => return Ok(None),
                other => return Err(anyhow!("frame inesperado: {other:?}")),
            }
        }
    }

    /// Wrapper conveniente: manda un texto.
    pub async fn send_text(&mut self, identity: &Identity, text: &str) -> Result<()> {
        self.send_app(identity, &AppPayload::Text(text.to_string())).await
    }

    /// Wrapper conveniente: lee el siguiente AppPayload y solo retorna el texto si es Text.
    /// Si llega un payload no-text (File, etc.), lo descarta y sigue esperando.
    /// Para manejar otros payloads, usa [`recv_app`] directamente.
    pub async fn recv_text(&mut self, identity: &Identity) -> Result<Option<String>> {
        loop {
            match self.recv_app(identity).await? {
                Some(AppPayload::Text(t)) => return Ok(Some(t)),
                Some(AppPayload::File { filename, data }) => {
                    tracing::warn!(
                        "recv_text: descartando AppPayload::File '{filename}' ({} bytes)",
                        data.len()
                    );
                    continue;
                }
                Some(AppPayload::FileChunk { filename, chunk_idx, total_chunks, .. }) => {
                    tracing::warn!(
                        "recv_text: descartando FileChunk '{filename}' ({chunk_idx}/{total_chunks})"
                    );
                    continue;
                }
                None => return Ok(None),
            }
        }
    }

    /// Devuelve el group_id de la conversación (para persistir en el contact).
    pub fn group_id_bytes(&self) -> Vec<u8> {
        self.group.group_id().as_slice().to_vec()
    }

    /// Cierre limpio del stream (no espera al peer).
    pub async fn close(mut self) -> Result<()> {
        let _ = send_frame(&mut self.stream, &Frame::Bye).await;
        Ok(())
    }

    /// Cierre cooperativo: manda `Bye` y luego espera EOF del peer (timeout 5s).
    /// Usar tras `send_app` cuando el caller es one-shot (CLI `send`/`send-file`):
    /// sin esto, el `drop` del stream puede llegarle al peer ANTES de que termine
    /// de leer el último MlsMessage, y la conexión se aborta con "Circuit closed".
    pub async fn say_goodbye(mut self) -> Result<()> {
        let _ = send_frame(&mut self.stream, &Frame::Bye).await;
        let _ = tokio::time::timeout(
            Duration::from_secs(5),
            drain_until_eof(&mut self.stream),
        )
        .await;
        Ok(())
    }
}

fn hex_short(bytes: &[u8]) -> String {
    let n = bytes.len().min(6);
    let head: String = bytes[..n].iter().map(|b| format!("{b:02x}")).collect();
    format!("{head}…")
}

async fn drain_until_eof<S>(stream: &mut S) -> Result<()>
where
    S: AsyncRead + Unpin,
{
    let mut buf = [0u8; 256];
    loop {
        match stream.read(&mut buf).await {
            Ok(0) => return Ok(()),
            Ok(_) => continue,
            Err(_) => return Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    /// Helper: arranca handshake en ambos lados en paralelo. Devuelve
    /// `(conv_initiator, conv_acceptor)`.
    async fn handshake_pair(
        alice: &Identity,
        bob: &Identity,
    ) -> Result<(Conversation<tokio::io::DuplexStream>, Conversation<tokio::io::DuplexStream>)> {
        let (a_stream, b_stream) = duplex(1024 * 1024);
        let resolver = NoResume;
        let (a_res, b_res) = tokio::join!(
            Conversation::open(
                a_stream,
                alice,
                Role::Initiator,
                "alice.onion:1234",
                Some("bob.onion:1234"),
                &resolver,
            ),
            Conversation::open(
                b_stream,
                bob,
                Role::Acceptor,
                "bob.onion:1234",
                None,
                &resolver,
            ),
        );
        let (conv_a, _) = a_res?;
        let (conv_b, _) = b_res?;
        Ok((conv_a, conv_b))
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fresh_handshake_text_roundtrip() -> Result<()> {
        let alice = Identity::new("alice")?;
        let bob = Identity::new("bob")?;
        let (mut conv_a, mut conv_b) = handshake_pair(&alice, &bob).await?;
        assert_eq!(conv_a.group.epoch().as_u64(), 1);
        assert_eq!(conv_a.group.epoch(), conv_b.group.epoch());

        // A → B
        let send_a = conv_a.send_text(&alice, "hola bob");
        let recv_b = conv_b.recv_text(&bob);
        let (s, r) = tokio::join!(send_a, recv_b);
        s?;
        assert_eq!(r?, Some("hola bob".to_string()));

        // B → A
        let send_b = conv_b.send_text(&bob, "hola alice");
        let recv_a = conv_a.recv_text(&alice);
        let (s, r) = tokio::join!(send_b, recv_a);
        s?;
        assert_eq!(r?, Some("hola alice".to_string()));

        Ok(())
    }

    /// Flujo offline-invite (fase 5a): Alice arma un grupo, consume un KeyPackage
    /// de Bob (lo que en producción sale del relay del peer), hace `add_members`
    /// localmente y obtiene un Welcome bytes. Bob procesa el Welcome con
    /// `process_welcome_blob` y queda joineado al mismo `group_id`. Verificamos
    /// que después de eso, un Application message de Alice se descifra OK en Bob —
    /// confirma que ambos están en el mismo MLS state sin handshake live.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn offline_welcome_via_keypackage_pool() -> Result<()> {
        use crate::identity;
        use openmls::prelude::tls_codec::Serialize as _;
        use openmls::prelude::{KeyPackageIn, MlsMessageIn, ProcessedMessageContent, ProtocolVersion};
        use openmls_traits::OpenMlsProvider as _;

        let alice = Identity::new("alice")?;
        let bob = Identity::new("bob")?;

        // 1) Bob publica un KeyPackage (en producción iría al pool del relay).
        let bob_kp_bundle = bob.fresh_key_package()?;
        let bob_kp_bytes = bob_kp_bundle
            .key_package()
            .tls_serialize_detached()
            .map_err(|e| anyhow!("serialize KP: {e:?}"))?;

        // 2) Alice consume el KP, valida y hace add_members local.
        let bob_kp_in = KeyPackageIn::tls_deserialize_exact_bytes(&bob_kp_bytes)
            .map_err(|e| anyhow!("deserialize KP: {e:?}"))?
            .validate(alice.provider.crypto(), ProtocolVersion::Mls10)
            .map_err(|e| anyhow!("validate KP: {e:?}"))?;
        let mut alice_group = identity::create_group(&alice)?;
        let alice_group_id = alice_group.group_id().as_slice().to_vec();
        let (_commit, welcome_out, _info) = alice_group
            .add_members(&alice.provider, &alice.signer, &[bob_kp_in])
            .map_err(|e| anyhow!("add_members: {e:?}"))?;
        alice_group
            .merge_pending_commit(&alice.provider)
            .map_err(|e| anyhow!("merge: {e:?}"))?;
        let welcome_bytes = welcome_out
            .tls_serialize_detached()
            .map_err(|e| anyhow!("serialize Welcome: {e:?}"))?;

        // 3) Bob detecta que es un Welcome (sniff sin descifrar) y lo procesa.
        assert!(
            identity::blob_is_welcome(&welcome_bytes),
            "blob_is_welcome debe identificar el Welcome correctamente"
        );
        let bob_group_id = identity::process_welcome_blob(&bob, &welcome_bytes)?;
        assert_eq!(
            bob_group_id, alice_group_id,
            "Bob debe joinear al mismo group_id que Alice"
        );

        // 4) Sanity check end-to-end: Alice manda un Application msg, Bob lo descifra.
        let payload = AppPayload::Text("offline-invite OK".into());
        let mut payload_bytes = Vec::with_capacity(64);
        ciborium::ser::into_writer(&payload, &mut payload_bytes)?;
        let app_out = alice_group
            .create_message(&alice.provider, &alice.signer, &payload_bytes)
            .map_err(|e| anyhow!("create_message: {e:?}"))?;
        let app_blob = app_out
            .tls_serialize_detached()
            .map_err(|e| anyhow!("serialize app msg: {e:?}"))?;

        let bob_group = openmls::group::MlsGroup::load(
            bob.provider.storage(),
            &openmls::group::GroupId::from_slice(&bob_group_id),
        )
        .map_err(|e| anyhow!("load bob group: {e:?}"))?
        .ok_or_else(|| anyhow!("bob group no se encontró tras process_welcome"))?;

        let in_msg = MlsMessageIn::tls_deserialize_exact_bytes(&app_blob)
            .map_err(|e| anyhow!("deserialize app msg: {e:?}"))?;
        let proto: openmls::framing::ProtocolMessage = in_msg
            .try_into_protocol_message()
            .map_err(|_| anyhow!("frame no es ProtocolMessage"))?;
        let mut bob_group_mut = bob_group;
        let processed = bob_group_mut
            .process_message(&bob.provider, proto)
            .map_err(|e| anyhow!("process_message: {e:?}"))?;
        match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(app) => {
                let bytes = app.into_bytes();
                let payload: AppPayload = ciborium::de::from_reader(&bytes[..])?;
                match payload {
                    AppPayload::Text(t) => assert_eq!(t, "offline-invite OK"),
                    other => panic!("esperaba Text, llegó {other:?}"),
                }
            }
            other => panic!("esperaba ApplicationMessage, llegó {other:?}"),
        }
        Ok(())
    }

    /// Fase 5b: cuando A invita a Carol al grupo donde Bob ya está, A envía el
    /// Commit a Bob por relay (Bob offline). Cuando Bob hace poll, debe poder
    /// procesar ese blob como `InboundBlob::Commit`, mergear el StagedCommit, y
    /// quedar en el mismo epoch que A — para que mensajes posteriores del grupo
    /// (encriptados al nuevo árbol de miembros) le descifren.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn commit_via_blob_advances_member_epoch() -> Result<()> {
        use crate::identity;
        use openmls::prelude::tls_codec::Serialize as _;
        use openmls::prelude::{KeyPackageIn, ProtocolVersion};
        use openmls_traits::OpenMlsProvider as _;

        let alice = Identity::new("alice")?;
        let bob = Identity::new("bob")?;
        let carol = Identity::new("carol")?;

        // 1) Alice crea un grupo y agrega a Bob (vía Welcome offline).
        let bob_kp_bytes = bob
            .fresh_key_package()?
            .key_package()
            .tls_serialize_detached()
            .map_err(|e| anyhow!("serialize bob KP: {e:?}"))?;
        let bob_kp = KeyPackageIn::tls_deserialize_exact_bytes(&bob_kp_bytes)
            .map_err(|e| anyhow!("deserialize bob KP: {e:?}"))?
            .validate(alice.provider.crypto(), ProtocolVersion::Mls10)
            .map_err(|e| anyhow!("validate bob KP: {e:?}"))?;
        let mut alice_group = identity::create_group(&alice)?;
        let alice_group_id = alice_group.group_id().as_slice().to_vec();
        let (_commit_b, welcome_b, _info_b) = alice_group
            .add_members(&alice.provider, &alice.signer, &[bob_kp])
            .map_err(|e| anyhow!("add_members bob: {e:?}"))?;
        alice_group
            .merge_pending_commit(&alice.provider)
            .map_err(|e| anyhow!("merge bob: {e:?}"))?;
        let welcome_b_bytes = welcome_b
            .tls_serialize_detached()
            .map_err(|e| anyhow!("serialize welcome bob: {e:?}"))?;
        let bob_joined_gid = identity::process_welcome_blob(&bob, &welcome_b_bytes)?;
        assert_eq!(bob_joined_gid, alice_group_id);
        assert_eq!(alice_group.epoch().as_u64(), 1);

        // 2) Alice invita a Carol mientras Bob está offline. Esto produce un
        //    Commit que NO viaja en ningún Conversation activo — Alice lo serializa
        //    y lo deja en el queue del relay de Bob.
        let carol_kp_bytes = carol
            .fresh_key_package()?
            .key_package()
            .tls_serialize_detached()
            .map_err(|e| anyhow!("serialize carol KP: {e:?}"))?;
        let carol_kp = KeyPackageIn::tls_deserialize_exact_bytes(&carol_kp_bytes)
            .map_err(|e| anyhow!("deserialize carol KP: {e:?}"))?
            .validate(alice.provider.crypto(), ProtocolVersion::Mls10)
            .map_err(|e| anyhow!("validate carol KP: {e:?}"))?;
        let (commit_c, welcome_c, _info_c) = alice_group
            .add_members(&alice.provider, &alice.signer, &[carol_kp])
            .map_err(|e| anyhow!("add_members carol: {e:?}"))?;
        alice_group
            .merge_pending_commit(&alice.provider)
            .map_err(|e| anyhow!("merge carol: {e:?}"))?;
        let commit_c_bytes = commit_c
            .tls_serialize_detached()
            .map_err(|e| anyhow!("serialize commit: {e:?}"))?;
        let _welcome_c_bytes = welcome_c
            .tls_serialize_detached()
            .map_err(|e| anyhow!("serialize welcome carol: {e:?}"))?;
        assert_eq!(alice_group.epoch().as_u64(), 2);

        // 3) Bob recibe el Commit como blob (relay). El helper unificado debe:
        //    a) detectar que NO es Welcome (es un Commit dentro de PrivateMessage),
        //    b) cargar su MlsGroup, process_message → StagedCommit,
        //    c) merge_staged_commit → epoch ahora 2,
        //    d) reportar `InboundBlob::Commit { epoch: 2 }`.
        let processed = process_inbound_blob(&bob, &commit_c_bytes)?;
        match processed {
            InboundBlob::Commit { group_id, epoch } => {
                assert_eq!(group_id, alice_group_id, "group_id no coincide");
                assert_eq!(epoch, 2, "epoch debe avanzar a 2 tras Commit");
            }
            other => panic!("esperaba InboundBlob::Commit, llegó {other:?}"),
        }

        // 4) Sanity check: ahora Alice manda un Application message; Bob (epoch=2)
        //    debe poder descifrarlo. Si Bob hubiera quedado en epoch=1, fallaría.
        let payload = AppPayload::Text("post-commit ping".into());
        let mut payload_bytes = Vec::with_capacity(64);
        ciborium::ser::into_writer(&payload, &mut payload_bytes)?;
        let app_out = alice_group
            .create_message(&alice.provider, &alice.signer, &payload_bytes)
            .map_err(|e| anyhow!("create_message post-commit: {e:?}"))?;
        let app_blob = app_out
            .tls_serialize_detached()
            .map_err(|e| anyhow!("serialize app: {e:?}"))?;
        let processed = process_inbound_blob(&bob, &app_blob)?;
        match processed {
            InboundBlob::App {
                payload: AppPayload::Text(t),
                group_id,
            } => {
                assert_eq!(t, "post-commit ping");
                assert_eq!(group_id, alice_group_id);
            }
            other => panic!("esperaba InboundBlob::App(Text), llegó {other:?}"),
        }
        Ok(())
    }

    /// KAT del wire de AppPayload: pequeños vectores que validan que el CBOR
    /// del payload de aplicación no cambia entre versiones. Si esto rompe,
    /// peers viejos no descifran payloads nuevos (o al revés). Bumpear el
    /// protocolo conscientemente y actualizar.
    #[test]
    fn app_payload_text_canonical_cbor() {
        let p = AppPayload::Text("ok".into());
        let mut buf = Vec::new();
        ciborium::ser::into_writer(&p, &mut buf).unwrap();
        // ciborium externally-tagged: { "Text": "ok" }
        // 0xa1 = map(1), 0x64 = text(4) "Text", 0x62 = text(2) "ok"
        assert_eq!(
            buf,
            [0xa1, 0x64, b'T', b'e', b'x', b't', 0x62, b'o', b'k'],
            "CBOR de AppPayload::Text('ok') no es el esperado"
        );
        // Roundtrip:
        let de: AppPayload = ciborium::de::from_reader(&buf[..]).unwrap();
        match de {
            AppPayload::Text(t) => assert_eq!(t, "ok"),
            other => panic!("esperaba Text, llegó {other:?}"),
        }
    }

    #[test]
    fn app_payload_filechunk_roundtrip() {
        let p = AppPayload::FileChunk {
            file_id: vec![0x11, 0x22, 0x33, 0x44],
            filename: "archivo.bin".into(),
            total_chunks: 3,
            chunk_idx: 1,
            total_bytes: 12345,
            data: b"chunk-2-payload".to_vec(),
        };
        let mut buf = Vec::new();
        ciborium::ser::into_writer(&p, &mut buf).unwrap();
        let de: AppPayload = ciborium::de::from_reader(&buf[..]).unwrap();
        match de {
            AppPayload::FileChunk {
                file_id,
                filename,
                total_chunks,
                chunk_idx,
                total_bytes,
                data,
            } => {
                assert_eq!(file_id, vec![0x11, 0x22, 0x33, 0x44]);
                assert_eq!(filename, "archivo.bin");
                assert_eq!(total_chunks, 3);
                assert_eq!(chunk_idx, 1);
                assert_eq!(total_bytes, 12345);
                assert_eq!(data, b"chunk-2-payload");
            }
            other => panic!("esperaba FileChunk, llegó {other:?}"),
        }
    }

    /// Chunking de archivos > FILE_INLINE_THRESHOLD: split + record_file_chunk
    /// reconstruye los bytes originales, incluso si llegan en orden invertido.
    #[test]
    fn chunked_file_roundtrip_out_of_order() -> Result<()> {
        let tmp = std::env::temp_dir().join(format!(
            "balchat-chunk-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp)?;

        // Generar un payload de 25 MiB para forzar 4 chunks de 8 MiB.
        let big: Vec<u8> = (0..25 * 1024 * 1024).map(|i| (i % 256) as u8).collect();
        let chunks = split_file_into_chunks("video.mp4", &big);
        assert_eq!(chunks.len(), 4, "25 MiB / 8 MiB ⌈⌉ = 4 chunks");

        // Enviar en orden invertido para asegurarnos que el reensamblaje no
        // depende del orden de llegada.
        let mut completed: Option<Vec<u8>> = None;
        for chunk in chunks.iter().rev() {
            if let AppPayload::FileChunk {
                file_id,
                filename,
                chunk_idx,
                total_chunks,
                total_bytes,
                data,
            } = chunk
            {
                let outcome = record_file_chunk(
                    &tmp,
                    file_id,
                    filename,
                    *chunk_idx,
                    *total_chunks,
                    *total_bytes,
                    data,
                )?;
                match outcome {
                    ChunkOutcome::Pending(p) => {
                        assert!(p.received < p.total);
                    }
                    ChunkOutcome::Complete { filename, data } => {
                        assert_eq!(filename, "video.mp4");
                        assert_eq!(data.len(), big.len());
                        assert_eq!(data, big);
                        cleanup_chunk_spool(&tmp, file_id);
                        completed = Some(data);
                    }
                }
            }
        }
        assert!(completed.is_some(), "el último chunk debió completar");

        // Idempotencia: re-enviar el mismo chunk no rompe.
        if let AppPayload::FileChunk {
            file_id,
            filename,
            chunk_idx,
            total_chunks,
            total_bytes,
            data,
        } = &chunks[0]
        {
            // Como ya hicimos cleanup, el primer chunk crea el spool de nuevo
            // y queda Pending — eso valida la idempotencia del path "vuelve a
            // empezar" sin romper la API.
            let outcome = record_file_chunk(
                &tmp,
                file_id,
                filename,
                *chunk_idx,
                *total_chunks,
                *total_bytes,
                data,
            )?;
            assert!(matches!(outcome, ChunkOutcome::Pending(_)));
            cleanup_chunk_spool(&tmp, file_id);
        }
        std::fs::remove_dir_all(&tmp).ok();
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn file_payload_roundtrip() -> Result<()> {
        let alice = Identity::new("alice")?;
        let bob = Identity::new("bob")?;
        let (mut conv_a, mut conv_b) = handshake_pair(&alice, &bob).await?;

        let payload = AppPayload::File {
            filename: "secret.txt".into(),
            data: b"contenido del archivo".to_vec(),
        };
        let send_a = conv_a.send_app(&alice, &payload);
        let recv_b = conv_b.recv_app(&bob);
        let (s, r) = tokio::join!(send_a, recv_b);
        s?;
        match r? {
            Some(AppPayload::File { filename, data }) => {
                assert_eq!(filename, "secret.txt");
                assert_eq!(data, b"contenido del archivo");
            }
            other => panic!("esperaba File, llegó {other:?}"),
        }
        Ok(())
    }

    /// Cross-sign: si Initiator tiene `expected_pubkey` que NO coincide con la
    /// signing key real del Acceptor, el handshake debe fallar.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cross_sign_mismatch_aborts_handshake() -> Result<()> {
        struct WrongKeyResolver;
        impl ResumeResolver for WrongKeyResolver {
            fn group_id_for(&self, _: &str) -> Option<Vec<u8>> {
                None
            }
            fn expected_pubkey_for(&self, _: &str) -> Option<Vec<u8>> {
                Some(vec![0xaa; 32]) // signing key claramente equivocada
            }
        }

        let alice = Identity::new("alice")?;
        let bob = Identity::new("bob")?;
        let (a_stream, b_stream) = duplex(64 * 1024);

        let resolver_init = WrongKeyResolver;
        let resolver_acc = NoResume;

        let (a_res, _b_res) = tokio::join!(
            Conversation::open(
                a_stream,
                &alice,
                Role::Initiator,
                "alice.onion:1234",
                Some("bob.onion:1234"),
                &resolver_init,
            ),
            Conversation::open(
                b_stream,
                &bob,
                Role::Acceptor,
                "bob.onion:1234",
                None,
                &resolver_acc,
            ),
        );
        let err = match a_res {
            Err(e) => e,
            Ok(_) => panic!("Initiator debió fallar por pubkey mismatch"),
        };
        assert!(
            format!("{err:#}").contains("pubkey del peer NO coincide"),
            "mensaje inesperado: {err:#}"
        );
        Ok(())
    }
}

/// Resuelve preguntas de resume y verificación:
///   * `group_id_for(peer_onion)` — el `mls_group_id` 1:1 que tengo con este peer.
///   * `knows_group_id(group_id)`  — ¿conozco este `mls_group_id`? (cubre tanto
///     grupos 1:1 indexados por onion como grupos n-way en la tabla `groups`).
///   * `expected_pubkey_for(peer_onion)` — la signing key MLS que esperamos para este
///     peer (configurada vía `add-contact --pubkey`); si está set, fresh handshakes
///     rechazan KeyPackages firmados con otra clave.
///
/// El CLI implementa esto sobre el Vault SQLCipher.
pub trait ResumeResolver: Send + Sync {
    fn group_id_for(&self, peer_onion: &str) -> Option<Vec<u8>>;

    /// Default: no grupos conocidos. Override para soportar grupos n-way.
    fn knows_group_id(&self, _group_id: &[u8]) -> bool {
        false
    }

    /// Default: sin pubkey esperada (TOFU). Override para validación cross-sign.
    fn expected_pubkey_for(&self, _peer_onion: &str) -> Option<Vec<u8>> {
        None
    }
}

/// Resolver no-op (siempre retorna None) — útil para tests sin storage.
pub struct NoResume;
impl ResumeResolver for NoResume {
    fn group_id_for(&self, _peer_onion: &str) -> Option<Vec<u8>> {
        None
    }
}

/// Resultado de procesar un blob MLS recibido desde el wire (relay o conexión
/// directa one-shot, sin un `Conversation` activo). Unifica los tres casos
/// posibles: Welcome (joineamos un grupo nuevo), Application (mensaje de chat),
/// Commit (alguien invitó a otro miembro y nosotros tenemos que avanzar epoch).
#[derive(Debug)]
pub enum InboundBlob {
    /// Llegó un `MlsMessage::Welcome`: ya joineamos el grupo. El caller suele
    /// querer registrarlo en su vault local.
    Welcome { group_id: Vec<u8> },
    /// Application message descifrado.
    App {
        payload: AppPayload,
        group_id: Vec<u8>,
    },
    /// `StagedCommitMessage` aplicado: epoch del grupo avanzó. Pasa cuando otro
    /// miembro invitó/expulsó a alguien y nos disemina el Commit. El blob ya
    /// se mergeó en el `MlsGroup` persistido — el caller solo tiene que
    /// `identity::save` y opcionalmente actualizar membresía si la lleva aparte.
    Commit { group_id: Vec<u8>, epoch: u64 },
}

/// Procesa un blob MLS recibido del wire (típicamente vía `RelayClient::get`)
/// sin requerir un `Conversation` activo. Detecta automáticamente si es Welcome,
/// Application message o Commit, aplica los efectos sobre el `MlsGroup` (joinear /
/// descifrar / mergear) y devuelve el outcome.
///
/// Para Welcome y Commit el state MLS queda mutado en el storage del provider —
/// llamar `identity::save(vault, identity)` después para persistirlo en el vault
/// SQLCipher.
pub fn process_inbound_blob(identity: &Identity, blob: &[u8]) -> Result<InboundBlob> {
    if crate::identity::blob_is_welcome(blob) {
        let group_id = crate::identity::process_welcome_blob(identity, blob)?;
        return Ok(InboundBlob::Welcome { group_id });
    }

    let in_msg = MlsMessageIn::tls_deserialize_exact_bytes(blob)
        .context("deserializar MlsMessageIn")?;
    let proto: ProtocolMessage = in_msg
        .try_into_protocol_message()
        .map_err(|_| anyhow!("frame MLS no es ProtocolMessage"))?;
    let group_id = proto.group_id().as_slice().to_vec();
    let mut group = MlsGroup::load(
        identity.provider.storage(),
        &GroupId::from_slice(&group_id),
    )
    .map_err(|e| anyhow!("MlsGroup::load: {e:?}"))?
    .ok_or_else(|| anyhow!("group_id del blob no en mi storage MLS"))?;
    let processed = group
        .process_message(&identity.provider, proto)
        .context("process_message")?;
    match processed.into_content() {
        ProcessedMessageContent::ApplicationMessage(app) => {
            let bytes = app.into_bytes();
            // Compat con texto plano legacy (pre-CBOR): si CBOR falla, tratamos como UTF-8.
            let payload = match ciborium::de::from_reader::<AppPayload, _>(&bytes[..]) {
                Ok(p) => p,
                Err(_) => AppPayload::Text(String::from_utf8_lossy(&bytes).into_owned()),
            };
            Ok(InboundBlob::App { payload, group_id })
        }
        ProcessedMessageContent::StagedCommitMessage(staged) => {
            group
                .merge_staged_commit(&identity.provider, *staged)
                .context("merge_staged_commit")?;
            let epoch = group.epoch().as_u64();
            Ok(InboundBlob::Commit { group_id, epoch })
        }
        other => Err(anyhow!(
            "contenido MLS inesperado en blob (no App ni Commit ni Welcome): {other:?}"
        )),
    }
}

/// Initiator side: invita a un peer a un grupo MLS YA existente. Devuelve el
/// `Commit` que el caller debe diseminar a los otros miembros para que actualicen
/// su epoch. Si el grupo solo te incluía a ti, no hay otros miembros y el caller
/// puede descartar el Commit.
pub async fn invite_peer_to_existing_group<S>(
    mut stream: S,
    identity: &Identity,
    our_onion: &str,
    group: &mut MlsGroup,
    expected_pubkey: Option<&[u8]>,
) -> Result<openmls::framing::MlsMessageOut>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    // Hello mutuo (sin resume — invitar siempre es handshake nuevo para el peer).
    send_frame(
        &mut stream,
        &Frame::Hello {
            protocol_version: PROTOCOL_VERSION,
            my_onion: our_onion.to_string(),
            resume_group_id: None,
        },
    )
    .await?;
    let peer_hello = recv_frame(&mut stream).await?;
    if !matches!(peer_hello, Frame::Hello { protocol_version, .. } if protocol_version == PROTOCOL_VERSION) {
        return Err(anyhow!("Hello inválido del peer durante invite"));
    }

    // Esperamos KeyPackage del peer (mismo flow que handshake 1:1).
    let peer_kp_bytes = match recv_frame(&mut stream).await? {
        Frame::KeyPackage(b) => b,
        other => return Err(anyhow!("esperaba KeyPackage, llegó {other:?}")),
    };
    let peer_kp = KeyPackageIn::tls_deserialize_exact_bytes(&peer_kp_bytes)
        .context("deserializar KeyPackage del peer (invite)")?
        .validate(identity.provider.crypto(), ProtocolVersion::Mls10)
        .context("validar KeyPackage del peer (invite)")?;

    if let Some(expected) = expected_pubkey {
        let actual = peer_kp.leaf_node().signature_key().as_slice();
        if actual != expected {
            return Err(anyhow!(
                "pubkey del peer NO coincide con --pubkey esperado durante invite"
            ));
        }
    }

    // add_members extiende el grupo existente.
    let (commit, welcome_out, _group_info) = group
        .add_members(&identity.provider, &identity.signer, &[peer_kp])
        .context("add_members al grupo existente")?;
    group
        .merge_pending_commit(&identity.provider)
        .context("merge_pending_commit (invite)")?;

    // Mandamos Welcome al new peer en este mismo stream.
    let welcome_bytes = welcome_out
        .tls_serialize_detached()
        .context("serializar Welcome (invite)")?;
    send_frame(&mut stream, &Frame::MlsMessage(welcome_bytes)).await?;

    // Cooperative close (el peer leerá Welcome, hará StagedWelcome, luego cerrará).
    let _ = send_frame(&mut stream, &Frame::Bye).await;
    let _ = tokio::time::timeout(Duration::from_secs(5), drain_until_eof(&mut stream)).await;

    Ok(commit)
}

/// Envía un único `MlsMessage` (ya serializado) a un miembro de un grupo n-way,
/// usando la conexión Tor abierta `stream`. Útil para diseminar Commits tras un
/// `add_members` (cuando hay 3+ miembros y los existing necesitan actualizar epoch).
///
/// Hello declara `resume_group_id = Some(group_id)`; el peer del otro lado
/// debe conocer ese group_id (lo verifica via `ResumeResolver::knows_group_id`).
pub async fn push_message_to_group_member<S>(
    mut stream: S,
    our_onion: &str,
    group_id: &[u8],
    mls_message_blob: Vec<u8>,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    send_frame(
        &mut stream,
        &Frame::Hello {
            protocol_version: PROTOCOL_VERSION,
            my_onion: our_onion.to_string(),
            resume_group_id: Some(group_id.to_vec()),
        },
    )
    .await?;
    let peer_hello = recv_frame(&mut stream).await?;
    let peer_resume = match peer_hello {
        Frame::Hello {
            resume_group_id, ..
        } => resume_group_id,
        other => return Err(anyhow!("se esperaba Hello, llegó {other:?}")),
    };
    if peer_resume.as_deref() != Some(group_id) {
        return Err(anyhow!(
            "peer no reconoce este group_id — no podemos entregarle el mensaje"
        ));
    }

    send_frame(&mut stream, &Frame::MlsMessage(mls_message_blob)).await?;
    let _ = send_frame(&mut stream, &Frame::Bye).await;
    let _ = tokio::time::timeout(
        Duration::from_secs(5),
        drain_until_eof(&mut stream),
    )
    .await;
    Ok(())
}

async fn fresh_handshake_initiator<S>(
    stream: &mut S,
    identity: &Identity,
    expected_pubkey: Option<&[u8]>,
) -> Result<MlsGroup>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    // Esperamos KeyPackage del peer (el acceptor lo manda).
    let peer_kp_bytes = match recv_frame(stream).await? {
        Frame::KeyPackage(b) => b,
        other => return Err(anyhow!("esperaba KeyPackage, llegó {other:?}")),
    };
    let peer_kp = KeyPackageIn::tls_deserialize_exact_bytes(&peer_kp_bytes)
        .context("deserializar KeyPackage del peer")?
        .validate(identity.provider.crypto(), ProtocolVersion::Mls10)
        .context("validar KeyPackage del peer")?;

    // Cross-sign opcional: si `add-contact --pubkey` registró la signing key
    // esperada, abortamos si no coincide con la que vino en el KeyPackage.
    if let Some(expected) = expected_pubkey {
        let actual = peer_kp.leaf_node().signature_key().as_slice();
        if actual != expected {
            return Err(anyhow!(
                "pubkey del peer NO coincide con la esperada: esperada {} != recibida {}",
                hex_short(expected),
                hex_short(actual)
            ));
        }
        tracing::info!("cross-sign OK: peer pubkey {} verificada", hex_short(expected));
    }

    let create_cfg = MlsGroupCreateConfig::builder()
        .use_ratchet_tree_extension(true)
        .build();
    let mut group = MlsGroup::new(
        &identity.provider,
        &identity.signer,
        &create_cfg,
        identity.credential.clone(),
    )
    .context("MlsGroup::new")?;

    let (_commit, welcome_out, _info) = group
        .add_members(&identity.provider, &identity.signer, &[peer_kp])
        .context("add_members")?;
    group
        .merge_pending_commit(&identity.provider)
        .context("merge_pending_commit")?;

    let welcome_bytes = welcome_out
        .tls_serialize_detached()
        .context("serializar Welcome")?;
    send_frame(stream, &Frame::MlsMessage(welcome_bytes)).await?;
    Ok(group)
}

async fn fresh_handshake_acceptor<S>(stream: &mut S, identity: &Identity) -> Result<MlsGroup>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    let kp_bundle = identity.fresh_key_package()?;
    let kp_bytes = kp_bundle
        .key_package()
        .tls_serialize_detached()
        .context("serializar KeyPackage propio")?;
    send_frame(stream, &Frame::KeyPackage(kp_bytes)).await?;

    let welcome_bytes = match recv_frame(stream).await? {
        Frame::MlsMessage(b) => b,
        other => return Err(anyhow!("esperaba Welcome, llegó {other:?}")),
    };
    let in_msg = MlsMessageIn::tls_deserialize_exact_bytes(&welcome_bytes)
        .context("deserializar Welcome")?;
    let welcome = match in_msg.extract() {
        MlsMessageBodyIn::Welcome(w) => w,
        other => return Err(anyhow!("se esperaba Welcome, llegó {other:?}")),
    };
    let staged = StagedWelcome::new_from_welcome(
        &identity.provider,
        &MlsGroupJoinConfig::default(),
        welcome,
        None,
    )
    .context("StagedWelcome::new_from_welcome")?;
    let group = staged
        .into_group(&identity.provider)
        .context("StagedWelcome::into_group")?;
    Ok(group)
}
