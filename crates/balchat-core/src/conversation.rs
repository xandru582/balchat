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
    /// Archivo entero in-line (límite real ~15 MB tras overhead MLS).
    File {
        filename: String,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    },
}

impl AppPayload {
    pub fn text(s: impl Into<String>) -> Self {
        AppPayload::Text(s.into())
    }
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
