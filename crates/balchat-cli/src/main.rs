//! balchat CLI — Fase 2c.
//!
//! Subcomandos:
//!   * `init`            — crea vault + identidad MLS persistente.
//!   * `my-id`           — muestra tu `.onion` + queue_id + relay (lo que un peer necesita).
//!   * `set-my-relay`    — fija el relay donde recibirás mensajes offline.
//!   * `host`            — levanta tu onion y espera UNA conexión (REPL chat).
//!   * `connect <ref>`   — dial directo + REPL chat (resume si hubo handshake antes).
//!   * `send <ref> <txt>`— manda 1 mensaje al relay del peer (offline-friendly).
//!   * `poll`            — descarga y descifra mensajes pendientes desde tu relay.
//!   * `add-contact`     — registra peer (label, onion, --relay, --queue).
//!   * `list-contacts`   — muestra contactos.

use anyhow::{anyhow, bail, Context, Result};
use balchat_core::conversation::{
    invite_peer_to_existing_group, push_message_to_group_member, AppPayload, Conversation,
    HandshakeOutcome, ResumeResolver, Role,
};
use balchat_core::relay_client::RelayClient;
use balchat_core::{identity, DataStream, Endpoint, Identity};
use balchat_storage::{Contact, Vault};
use clap::{Parser, Subcommand};
use openmls::prelude::tls_codec::Serialize as _;
use openmls::prelude::*;
use openmls_traits::OpenMlsProvider;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};

const VIRTUAL_PORT: u16 = 1234;
const DEFAULT_VAULT: &str = "~/.balchat/vault.db";
const DEFAULT_NICKNAME: &str = "balchat";
const VAULT_KEY_MY_ONION: &str = "my_onion.v1";

#[derive(Parser)]
#[command(name = "balchat", about = "Chat 1:1 cifrado E2E sobre Tor (Fase 2c)")]
struct Cli {
    #[arg(long, global = true, default_value = DEFAULT_VAULT, value_name = "PATH")]
    vault: String,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Crea vault + identidad MLS persistente.
    Init {
        #[arg(long, default_value = "me")]
        label: String,
    },
    /// Muestra tu identidad pública (lo que un peer necesita para añadirte).
    MyId,
    /// Fija el relay donde TÚ vas a recibir mensajes offline.
    SetMyRelay { relay_onion: String },
    /// Levanta tu onion y espera una conexión.
    Host {
        #[arg(long, default_value = DEFAULT_NICKNAME)]
        nickname: String,
    },
    /// Dial directo a un peer + REPL chat.
    Connect {
        target: String,
        #[arg(long, default_value = DEFAULT_NICKNAME)]
        nickname: String,
    },
    /// Manda un texto al peer (intenta directo; cae a relay si offline).
    Send {
        target: String,
        text: String,
    },
    /// Manda un archivo al peer (mismo path que send: directo > relay).
    SendFile {
        target: String,
        path: String,
    },
    /// Descarga mensajes pendientes desde tu relay y los descifra (one-shot).
    Poll {
        #[arg(long, default_value_t = 64)]
        max: u32,
    },
    /// Daemon: poll periódico al relay + (opcional) acepta conexiones entrantes live.
    Watch {
        #[arg(long, default_value_t = 30)]
        interval: u64,
        #[arg(long, default_value_t = 64)]
        max: u32,
        /// Levanta también tu onion para aceptar conexiones directas mientras polea.
        #[arg(long)]
        listen: bool,
        #[arg(long, default_value = DEFAULT_NICKNAME)]
        nickname: String,
    },
    /// Registra un contacto.
    AddContact {
        label: String,
        onion: String,
        /// Relay del peer (donde dejar mensajes offline para él).
        #[arg(long)]
        relay: Option<String>,
        /// Queue id del peer en su relay (hex de 32 bytes = 64 chars).
        #[arg(long)]
        queue: Option<String>,
        /// Signing key MLS esperada del peer (cross-sign opcional, hex). Si la setás,
        /// el handshake fresh aborta si el peer firma con otra key.
        #[arg(long)]
        pubkey: Option<String>,
    },
    /// Lista contactos.
    ListContacts,
    /// Crea un grupo MLS n-way (sólo tú dentro inicialmente).
    CreateGroup { label: String },
    /// Lista los grupos creados.
    Groups,
    /// Bootstrap 1:1 con un peer SIN requerir que esté online: consume un KeyPackage
    /// del relay del peer, genera Welcome, y lo deja en su queue. El peer joineará
    /// el grupo cuando haga `poll`/`watch`.
    #[command(name = "bootstrap-1to1")]
    Bootstrap1to1 {
        target: String,
    },
    /// Publica N KeyPackages frescos en tu relay (pool para que peers offline te
    /// puedan invitar). Llamado automáticamente por `watch --listen`.
    #[command(name = "publish-kp")]
    PublishKp {
        #[arg(long, default_value_t = 10)]
        count: u32,
    },
    /// Invita un peer a un grupo. Requiere que el peer esté online (handshake live).
    Invite {
        group: String,
        target: String,
    },
    /// Manda un texto a todos los miembros del grupo (multicast directo + relay).
    SendGroup {
        group: String,
        text: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,arti=warn,tor_=warn")),
        )
        .init();

    let cli = Cli::parse();
    let vault_path = expand_tilde(&cli.vault);

    match cli.cmd {
        Cmd::Init { label } => init(&vault_path, &label),
        Cmd::MyId => my_id(&vault_path),
        Cmd::SetMyRelay { relay_onion } => set_my_relay(&vault_path, &relay_onion),
        Cmd::Host { nickname } => run_host(&vault_path, &nickname).await,
        Cmd::Connect { target, nickname } => run_connect(&vault_path, &target, &nickname).await,
        Cmd::Send { target, text } => {
            run_send_payload(&vault_path, &target, AppPayload::Text(text)).await
        }
        Cmd::SendFile { target, path } => {
            let p = expand_tilde(&path);
            let data = std::fs::read(&p).with_context(|| format!("leer {}", p.display()))?;
            let filename = p
                .file_name()
                .ok_or_else(|| anyhow!("path sin nombre de archivo"))?
                .to_string_lossy()
                .to_string();
            println!("[balchat] enviando archivo {filename} ({} bytes)", data.len());
            run_send_payload(
                &vault_path,
                &target,
                AppPayload::File { filename, data },
            )
            .await
        }
        Cmd::Poll { max } => run_poll(&vault_path, max).await,
        Cmd::Watch {
            interval,
            max,
            listen,
            nickname,
        } => run_watch(&vault_path, interval, max, listen, &nickname).await,
        Cmd::AddContact {
            label,
            onion,
            relay,
            queue,
            pubkey,
        } => add_contact(
            &vault_path,
            &label,
            &onion,
            relay.as_deref(),
            queue.as_deref(),
            pubkey.as_deref(),
        ),
        Cmd::ListContacts => list_contacts(&vault_path),
        Cmd::CreateGroup { label } => create_group(&vault_path, &label),
        Cmd::Groups => list_groups(&vault_path),
        Cmd::Invite { group, target } => run_invite(&vault_path, &group, &target).await,
        Cmd::SendGroup { group, text } => run_send_group(&vault_path, &group, &text).await,
        Cmd::Bootstrap1to1 { target } => run_bootstrap_1to1(&vault_path, &target).await,
        Cmd::PublishKp { count } => run_publish_kp(&vault_path, count).await,
    }
}

// ---------- comandos sin red ----------

fn init(vault_path: &Path, label: &str) -> Result<()> {
    if vault_path.exists() {
        bail!("el vault {} ya existe", vault_path.display());
    }
    println!("[balchat] creando vault en {}", vault_path.display());
    let passphrase = read_passphrase("Passphrase nueva: ", true)?;
    let vault = Vault::open(vault_path, &passphrase)?;
    let id = identity::load_or_create(&vault, label)?;
    let queue_id = identity::load_or_create_queue_id(&vault)?;
    println!(
        "[balchat] identidad lista. signature_pubkey={}, queue={}",
        hex_short(id.signer.public()),
        hex_short(&queue_id)
    );
    println!("[balchat] siguiente: `balchat host` (genera tu .onion) y `balchat my-id`.");
    Ok(())
}

fn my_id(vault_path: &Path) -> Result<()> {
    let (vault, id, _) = open_vault_and_identity(vault_path)?;
    let onion = vault
        .kv_get(VAULT_KEY_MY_ONION)?
        .map(|b| String::from_utf8_lossy(&b).into_owned());
    let queue = identity::load_or_create_queue_id(&vault)?;
    let relay = identity::get_my_relay(&vault)?;

    println!("=== balchat id card ===");
    println!("ONION  : {}", onion.unwrap_or_else(|| "(corre `balchat host` una vez)".into()));
    println!("QUEUE  : {}", hex::encode(&queue));
    println!("PUBKEY : {}", hex::encode(id.signer.public()));
    println!("RELAY  : {}", relay.unwrap_or_else(|| "(no configurado — usa `set-my-relay`)".into()));
    println!();
    println!("Para que un peer te añada como contacto:");
    println!("  balchat add-contact <label> <ONION> --queue <QUEUE> --relay <RELAY>");
    println!();
    println!("(la PUBKEY es para verificación opcional fuera de banda — confirma que tu peer tiene esta misma signature key MLS)");
    Ok(())
}

fn set_my_relay(vault_path: &Path, relay_onion: &str) -> Result<()> {
    let (vault, _id, _) = open_vault_and_identity(vault_path)?;
    identity::set_my_relay(&vault, relay_onion)?;
    println!("[balchat] my relay = {relay_onion}");
    Ok(())
}

fn add_contact(
    vault_path: &Path,
    label: &str,
    onion: &str,
    relay: Option<&str>,
    queue_hex: Option<&str>,
    pubkey_hex: Option<&str>,
) -> Result<()> {
    let (vault, _id, _) = open_vault_and_identity(vault_path)?;
    let normalized = normalize_onion(onion);
    let queue_id = match queue_hex {
        Some(s) => Some(hex::decode(s).context("queue debe ser hex")?),
        None => None,
    };
    if let Some(q) = &queue_id {
        if q.len() != 32 {
            bail!("queue_id debe ser 32 bytes (64 hex chars), recibido {} bytes", q.len());
        }
    }
    let expected_pubkey = match pubkey_hex {
        Some(s) => Some(hex::decode(s).context("pubkey debe ser hex")?),
        None => None,
    };
    if let Some(p) = &expected_pubkey {
        if p.is_empty() || p.len() > 128 {
            bail!("pubkey de longitud inesperada: {} bytes", p.len());
        }
    }
    vault.upsert_contact(&Contact {
        label: label.to_string(),
        onion_address: normalized.clone(),
        relay_onion: relay.map(String::from),
        relay_queue_id: queue_id,
        expected_pubkey,
        ..Default::default()
    })?;
    println!("[balchat] contacto guardado: {label} → {normalized}");
    Ok(())
}

fn list_contacts(vault_path: &Path) -> Result<()> {
    let (vault, _id, _) = open_vault_and_identity(vault_path)?;
    let contacts = vault.list_contacts()?;
    if contacts.is_empty() {
        println!("(sin contactos)");
        return Ok(());
    }
    for c in contacts {
        let group = if c.mls_group_id.is_some() { "[grupo]" } else { "[no-grupo]" };
        let queue = c.relay_queue_id.as_ref().map(|q| hex_short(q)).unwrap_or_else(|| "(no queue)".into());
        let relay = c.relay_onion.as_deref().unwrap_or("(no relay)");
        println!("  {:14}  {}  {}  queue={}  relay={}", c.label, c.onion_address, group, queue, relay);
    }
    Ok(())
}

// ---------- comandos online (host / connect) ----------

async fn run_host(vault_path: &Path, nickname: &str) -> Result<()> {
    let (vault, identity, base_dir) = open_vault_and_identity(vault_path)?;

    println!("[balchat] bootstrap Arti...");
    let endpoint = Endpoint::bootstrap_in(&base_dir).await?;

    println!("[balchat] levantando onion '{nickname}'...");
    let mut handle = endpoint.host_onion(nickname).await?;
    let our_onion_with_port = format!("{}:{}", handle.onion, VIRTUAL_PORT);
    println!("[balchat] tu dirección: {our_onion_with_port}");

    // Persistir my_onion para que `my-id` la conozca.
    vault.kv_set(VAULT_KEY_MY_ONION, our_onion_with_port.as_bytes())?;

    println!("[balchat] esperando peer...");
    let stream = handle
        .incoming
        .recv()
        .await
        .ok_or_else(|| anyhow!("canal de incoming cerrado"))?;
    println!("[balchat] peer conectado, handshake...");

    let resolver = VaultResolver::new(&vault);
    let (conv, outcome) = Conversation::open(
        stream,
        &identity,
        Role::Acceptor,
        &our_onion_with_port,
        None,
        &resolver,
    )
    .await?;

    handle_outcome(&vault, &outcome)?;
    identity::save(&vault, &identity)?;

    println!("[balchat] handshake OK ({}). Empieza a chatear.\n", outcome_summary(&outcome));
    let result = chat_repl(conv, &identity, &vault).await;
    drop(handle);
    result
}

async fn run_connect(vault_path: &Path, target: &str, nickname: &str) -> Result<()> {
    let (vault, identity, base_dir) = open_vault_and_identity(vault_path)?;
    let peer_onion = resolve_target_to_onion(&vault, target)?;
    let dial_target = if peer_onion.contains(':') {
        peer_onion.clone()
    } else {
        format!("{peer_onion}:{VIRTUAL_PORT}")
    };

    println!("[balchat] bootstrap Arti...");
    let endpoint = Endpoint::bootstrap_in(&base_dir).await?;

    println!("[balchat] levantando tu onion '{nickname}'...");
    let handle = endpoint.host_onion(nickname).await?;
    let our_onion_with_port = format!("{}:{}", handle.onion, VIRTUAL_PORT);
    println!("[balchat] tu dirección: {our_onion_with_port}");
    vault.kv_set(VAULT_KEY_MY_ONION, our_onion_with_port.as_bytes())?;

    println!("[balchat] dial a {dial_target}...");
    let stream = retry_dial(&endpoint, &dial_target, 5).await?;
    println!("[balchat] conectado, handshake...");

    let resolver = VaultResolver::new(&vault);
    let (conv, outcome) = Conversation::open(
        stream,
        &identity,
        Role::Initiator,
        &our_onion_with_port,
        Some(peer_onion.as_str()),
        &resolver,
    )
    .await?;

    handle_outcome(&vault, &outcome)?;
    identity::save(&vault, &identity)?;

    println!("[balchat] handshake OK ({}). Empieza a chatear.\n", outcome_summary(&outcome));
    let result = chat_repl(conv, &identity, &vault).await;
    drop(handle);
    result
}

// ---------- send / poll vía relay ----------

async fn run_send_payload(vault_path: &Path, target: &str, payload: AppPayload) -> Result<()> {
    let (vault, identity, base_dir) = open_vault_and_identity(vault_path)?;

    let peer_onion = resolve_target_to_onion(&vault, target)?;
    let contact = vault
        .get_contact_by_onion(&peer_onion)?
        .ok_or_else(|| anyhow!("contacto desconocido: {peer_onion}"))?;
    if contact.mls_group_id.is_none() {
        bail!(
            "no hay grupo MLS con {peer_onion} — primero hacé handshake con `connect` o `host`"
        );
    }

    println!("[balchat] bootstrap Arti...");
    let endpoint = Endpoint::bootstrap_in(&base_dir).await?;

    let dial_target = if peer_onion.contains(':') {
        peer_onion.clone()
    } else {
        format!("{peer_onion}:{VIRTUAL_PORT}")
    };

    // 1. Try directo (timeout corto). Si peer corre `watch --listen` o `host`, este path
    //    es órdenes de magnitud más rápido que ir por relay y mantiene PCS continuo.
    println!("[balchat] intento dial directo a {dial_target} (60s timeout)...");
    let direct = tokio::time::timeout(Duration::from_secs(60), endpoint.dial(&dial_target)).await;
    match direct {
        Ok(Ok(stream)) => {
            println!("[balchat] dial directo OK, enviando vía Conversation MLS...");
            send_via_direct_stream(stream, &identity, &vault, &peer_onion, &payload).await?;
            println!("[balchat] entregado directo (peer online)");
            return Ok(());
        }
        Ok(Err(e)) => {
            println!("[balchat] dial directo falló: {e:#} → relay fallback");
        }
        Err(_) => {
            println!("[balchat] dial directo timeout 60s → relay fallback");
        }
    }

    // 2. Fallback al relay del peer.
    let relay_onion = contact
        .relay_onion
        .ok_or_else(|| anyhow!("contacto sin --relay configurado y peer no responde directo"))?;
    let queue_id = contact
        .relay_queue_id
        .ok_or_else(|| anyhow!("contacto sin --queue configurado"))?;
    let group_id = contact.mls_group_id.expect("ya validado arriba");

    let mut group = MlsGroup::load(identity.provider.storage(), &GroupId::from_slice(&group_id))
        .map_err(|e| anyhow!("MlsGroup::load: {e:?}"))?
        .ok_or_else(|| anyhow!("group_id en contacts no encontrado en MLS storage"))?;
    let mut payload_bytes = Vec::with_capacity(256);
    ciborium::ser::into_writer(&payload, &mut payload_bytes)
        .map_err(|e| anyhow!("serializar AppPayload: {e}"))?;
    let mls_out = group
        .create_message(&identity.provider, &identity.signer, &payload_bytes)
        .context("create_message (fallback)")?;
    let blob = mls_out.tls_serialize_detached().context("serializar MLS")?;
    identity::save(&vault, &identity)?;

    println!(
        "[balchat] PUT a relay {} (queue {})",
        relay_onion,
        hex_short(&queue_id)
    );
    let client = RelayClient::new(&endpoint);
    let seq = client.put(&relay_onion, &queue_id, blob).await?;
    println!("[balchat] mensaje en cola, seq={seq}");
    Ok(())
}

/// Sub-path "directo": tenemos un DataStream abierto al peer; hacemos handshake/resume,
/// mandamos un único Application message y cerramos.
async fn send_via_direct_stream(
    stream: DataStream,
    identity: &Identity,
    vault: &Vault,
    peer_onion: &str,
    payload: &AppPayload,
) -> Result<()> {
    let our_onion = vault
        .kv_get(VAULT_KEY_MY_ONION)?
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let resolver = VaultResolver::new(vault);
    let (mut conv, outcome) = Conversation::open(
        stream,
        identity,
        Role::Initiator,
        &our_onion,
        Some(peer_onion),
        &resolver,
    )
    .await?;
    handle_outcome(vault, &outcome)?;
    identity::save(vault, identity)?;
    conv.send_app(identity, payload).await?;
    identity::save(vault, identity)?;
    conv.say_goodbye().await?;
    Ok(())
}

async fn run_poll(vault_path: &Path, max: u32) -> Result<()> {
    let (vault, identity, base_dir) = open_vault_and_identity(vault_path)?;
    let (my_relay, my_queue) = my_relay_and_queue(&vault)?;
    println!(
        "[balchat] poll relay={my_relay} queue={} since_seq={}",
        hex_short(&my_queue),
        vault.get_last_seq(&my_relay, &my_queue)?
    );
    println!("[balchat] bootstrap Arti...");
    let endpoint = Endpoint::bootstrap_in(&base_dir).await?;
    let client = RelayClient::new(&endpoint);

    let n = poll_once(&client, &my_relay, &my_queue, &vault, &identity, max).await?;
    println!("[balchat] {n} mensaje(s) procesados");
    Ok(())
}

async fn run_watch(
    vault_path: &Path,
    interval_secs: u64,
    max: u32,
    listen: bool,
    nickname: &str,
) -> Result<()> {
    if interval_secs < 5 {
        bail!("interval mínimo razonable: 5s");
    }
    let (vault, identity, base_dir) = open_vault_and_identity(vault_path)?;
    let (my_relay, my_queue) = my_relay_and_queue(&vault)?;

    println!("[balchat] bootstrap Arti...");
    let endpoint = Endpoint::bootstrap_in(&base_dir).await?;
    let client = RelayClient::new(&endpoint);

    let (mut listen_handle, our_onion) = if listen {
        println!("[balchat] levantando onion '{nickname}' para aceptar conexiones entrantes...");
        let h = endpoint.host_onion(nickname).await?;
        let our = format!("{}:{}", h.onion, VIRTUAL_PORT);
        vault.kv_set(VAULT_KEY_MY_ONION, our.as_bytes())?;
        println!("[balchat] listening: {our}");
        (Some(h), our)
    } else {
        let saved = vault
            .kv_get(VAULT_KEY_MY_ONION)?
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        (None, saved)
    };

    // Publicar pool inicial de KeyPackages para que peers offline puedan iniciar
    // handshake contra mí. Solo si tengo my-relay configurado.
    if identity::get_my_relay(&vault)?.is_some() {
        match publish_kp_pool(&endpoint, &vault, &identity, 10).await {
            Ok(pool) => println!("[balchat] pool de KeyPackages en relay = {pool}"),
            Err(e) => tracing::warn!("publish_kp_pool falló: {e:#}"),
        }
    }

    println!(
        "[balchat] watching relay={my_relay} queue={} cada {interval_secs}s (Ctrl+C para parar)",
        hex_short(&my_queue),
    );

    if let Err(e) = poll_once(&client, &my_relay, &my_queue, &vault, &identity, max).await {
        tracing::warn!("poll inicial falló: {e:#}");
    }

    let interval = Duration::from_secs(interval_secs);
    loop {
        tokio::select! {
            _ = tokio::time::sleep(interval) => {
                if let Err(e) = poll_once(&client, &my_relay, &my_queue, &vault, &identity, max).await {
                    tracing::warn!("poll falló: {e:#}");
                }
            }
            maybe_stream = listen_recv(&mut listen_handle) => {
                if let Some(stream) = maybe_stream {
                    if let Err(e) = handle_incoming(stream, &identity, &vault, &our_onion).await {
                        tracing::warn!("conexión entrante falló: {e:#}");
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\n[balchat] saliendo (Ctrl+C)");
                return Ok(());
            }
        }
    }
}

/// Espera la siguiente conexión entrante. Si no estamos escuchando, este future
/// no se completa nunca (lo cual deja a select! ignorar la rama).
async fn listen_recv(handle: &mut Option<balchat_core::HostHandle>) -> Option<DataStream> {
    match handle {
        Some(h) => h.incoming.recv().await,
        None => std::future::pending().await,
    }
}

/// Acepta una conexión entrante: handshake (resume si corresponde) y luego
/// recibe Application messages hasta que el peer cierre.
async fn handle_incoming(
    stream: DataStream,
    identity: &Identity,
    vault: &Vault,
    our_onion: &str,
) -> Result<()> {
    let resolver = VaultResolver::new(vault);
    let (mut conv, outcome) = Conversation::open(
        stream,
        identity,
        Role::Acceptor,
        our_onion,
        None,
        &resolver,
    )
    .await?;
    handle_outcome(vault, &outcome)?;
    identity::save(vault, identity)?;
    tracing::info!(
        "conn entrante: peer={} mode={}",
        conv.peer_onion,
        outcome_summary(&outcome)
    );

    while let Some(payload) = conv.recv_app(identity).await? {
        let from = conv.peer_onion.clone();
        match payload {
            AppPayload::Text(t) => {
                tracing::info!("[de {from}] {t}");
            }
            AppPayload::File { filename, data } => {
                let inbox_dir = inbox_for(vault, &from);
                match save_to_inbox(&inbox_dir, &filename, &data) {
                    Ok(path) => tracing::info!(
                        "[de {from}] archivo recibido ({} bytes) → {}",
                        data.len(),
                        path.display()
                    ),
                    Err(e) => tracing::warn!("[de {from}] guardar archivo falló: {e:#}"),
                }
            }
        }
        identity::save(vault, identity)?;
    }
    Ok(())
}

fn inbox_for(vault: &Vault, peer_onion: &str) -> PathBuf {
    let base = vault
        .path()
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("inbox").join(sanitize_path_segment(peer_onion))
}

fn save_to_inbox(dir: &Path, filename: &str, data: &[u8]) -> Result<PathBuf> {
    std::fs::create_dir_all(dir).with_context(|| format!("crear {}", dir.display()))?;
    let safe = sanitize_path_segment(filename);
    let mut path = dir.join(&safe);
    // Si ya existe, agregamos sufijo numérico para no sobreescribir.
    let mut i = 1u32;
    while path.exists() {
        path = dir.join(format!("{safe}.{i}"));
        i += 1;
        if i > 1000 {
            bail!("demasiados archivos con nombre conflictivo en {}", dir.display());
        }
    }
    std::fs::write(&path, data).with_context(|| format!("escribir {}", path.display()))?;
    Ok(path)
}

fn sanitize_path_segment(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Hace un poll del relay y procesa mensajes. Devuelve cuántos se procesaron.
async fn poll_once(
    client: &RelayClient<'_>,
    relay: &str,
    queue_id: &[u8],
    vault: &Vault,
    identity: &Identity,
    max: u32,
) -> Result<usize> {
    let last_seq = vault.get_last_seq(relay, queue_id)?;
    let messages = client.get(relay, queue_id, last_seq, max).await?;
    if messages.is_empty() {
        return Ok(0);
    }

    let mut new_last = last_seq;
    for msg in &messages {
        // Detectar Welcomes (handshake offline iniciado por un peer): si el blob
        // es un Welcome, joinear el grupo y emitir info; si no, descifrar como hoy.
        if identity::blob_is_welcome(&msg.blob) {
            match identity::process_welcome_blob(identity, &msg.blob) {
                Ok(group_id) => {
                    identity::save(vault, identity).ok();
                    // Persistir el grupo con un label sintético si todavía no existe
                    // (para que aparezca en `balchat groups` y se pueda enviar a él).
                    if vault
                        .get_group_by_mls_id(&group_id)
                        .ok()
                        .flatten()
                        .is_none()
                    {
                        let label = format!("inbox-{}", hex_short_owned(&group_id).trim_end_matches('…'));
                        if let Err(e) = vault.create_group(&label, &group_id) {
                            tracing::warn!("registrar grupo offline en vault falló: {e:#}");
                        }
                    }
                    tracing::info!(
                        seq = msg.seq,
                        "[seq={}] joineado grupo MLS via Welcome offline (group_id={})",
                        msg.seq,
                        hex_short_owned(&group_id),
                    );
                }
                Err(e) => tracing::warn!(
                    seq = msg.seq,
                    "[seq={}] procesar Welcome falló: {e:#}",
                    msg.seq
                ),
            }
            if msg.seq > new_last {
                new_last = msg.seq;
            }
            continue;
        }
        match decrypt_blob(identity, &msg.blob) {
            Ok(AppPayload::Text(t)) => tracing::info!(seq = msg.seq, "[seq={}] {t}", msg.seq),
            Ok(AppPayload::File { filename, data }) => {
                let inbox_dir = inbox_for(vault, "from-relay");
                match save_to_inbox(&inbox_dir, &filename, &data) {
                    Ok(path) => tracing::info!(
                        seq = msg.seq,
                        "[seq={}] archivo {} ({} bytes) → {}",
                        msg.seq,
                        filename,
                        data.len(),
                        path.display()
                    ),
                    Err(e) => tracing::warn!(
                        seq = msg.seq,
                        "[seq={}] guardar archivo falló: {e:#}",
                        msg.seq
                    ),
                }
            }
            Err(e) => tracing::warn!(seq = msg.seq, "[seq={}] descifrado falló: {e:#}", msg.seq),
        }
        if msg.seq > new_last {
            new_last = msg.seq;
        }
    }
    vault.set_last_seq(relay, queue_id, new_last)?;
    identity::save(vault, identity)?;
    Ok(messages.len())
}

fn my_relay_and_queue(vault: &Vault) -> Result<(String, Vec<u8>)> {
    let relay = identity::get_my_relay(vault)?
        .ok_or_else(|| anyhow!("no tienes my-relay configurado (`set-my-relay`)"))?;
    let queue = identity::load_or_create_queue_id(vault)?;
    Ok((relay, queue))
}


fn decrypt_blob(identity: &Identity, blob: &[u8]) -> Result<AppPayload> {
    let in_msg = MlsMessageIn::tls_deserialize_exact_bytes(blob).context("deserializar MLS")?;
    let proto: ProtocolMessage = in_msg
        .try_into_protocol_message()
        .map_err(|_| anyhow!("frame no es ProtocolMessage"))?;
    let group_id = proto.group_id().clone();
    let mut group = MlsGroup::load(identity.provider.storage(), &group_id)
        .map_err(|e| anyhow!("MlsGroup::load: {e:?}"))?
        .ok_or_else(|| anyhow!("group_id del mensaje no está en mi storage"))?;
    let processed = group
        .process_message(&identity.provider, proto)
        .context("process_message")?;
    match processed.into_content() {
        ProcessedMessageContent::ApplicationMessage(app) => {
            let bytes = app.into_bytes();
            // Compat: bytes son CBOR de AppPayload, o (legacy) UTF-8 plano.
            match ciborium::de::from_reader::<AppPayload, _>(&bytes[..]) {
                Ok(p) => Ok(p),
                Err(_) => Ok(AppPayload::Text(String::from_utf8_lossy(&bytes).into_owned())),
            }
        }
        other => Err(anyhow!("contenido inesperado: {other:?}")),
    }
}

// ---------- Grupos n-way ----------

fn create_group(vault_path: &Path, label: &str) -> Result<()> {
    let (vault, identity, _) = open_vault_and_identity(vault_path)?;
    if vault.get_group_by_label(label)?.is_some() {
        bail!("el grupo '{label}' ya existe");
    }
    let group = identity::create_group(&identity)?;
    let group_id = group.group_id().as_slice().to_vec();
    vault.create_group(label, &group_id)?;
    identity::save(&vault, &identity)?;
    println!(
        "[balchat] grupo '{label}' creado (mls_group_id={})",
        hex_short(&group_id)
    );
    println!("[balchat] usa `balchat invite {label} <peer>` para añadir miembros.");
    Ok(())
}

fn list_groups(vault_path: &Path) -> Result<()> {
    let (vault, _id, _) = open_vault_and_identity(vault_path)?;
    let groups = vault.list_groups()?;
    if groups.is_empty() {
        println!("(sin grupos)");
        return Ok(());
    }
    for g in groups {
        let members = vault.list_group_members(&g.label).unwrap_or_default();
        println!(
            "  {:14}  id={}  miembros={}",
            g.label,
            hex_short(&g.mls_group_id),
            if members.is_empty() {
                "(solo yo)".to_string()
            } else {
                format!("{} ({})", members.len() + 1, members.join(","))
            }
        );
    }
    Ok(())
}

async fn run_invite(vault_path: &Path, group_label: &str, target: &str) -> Result<()> {
    let (vault, identity, base_dir) = open_vault_and_identity(vault_path)?;
    let group_record = vault
        .get_group_by_label(group_label)?
        .ok_or_else(|| anyhow!("grupo '{group_label}' no existe — `create-group` primero"))?;

    let peer_onion = resolve_target_to_onion(&vault, target)?;
    let existing_members = vault.list_group_members(group_label)?;
    if existing_members.iter().any(|m| m == &peer_onion) {
        bail!("'{peer_onion}' ya es miembro del grupo");
    }

    let mut group = identity::load_group(&identity, &group_record.mls_group_id)?;

    println!("[balchat] bootstrap Arti...");
    let endpoint = Endpoint::bootstrap_in(&base_dir).await?;
    let our_onion = vault
        .kv_get(VAULT_KEY_MY_ONION)?
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();

    let dial_target = if peer_onion.contains(':') {
        peer_onion.clone()
    } else {
        format!("{peer_onion}:{VIRTUAL_PORT}")
    };
    println!("[balchat] dial {dial_target} para invitar...");
    let stream = retry_dial(&endpoint, &dial_target, 3).await?;
    let expected_pubkey = vault
        .get_contact_by_onion(&peer_onion)?
        .and_then(|c| c.expected_pubkey);
    let commit = invite_peer_to_existing_group(
        stream,
        &identity,
        &our_onion,
        &mut group,
        expected_pubkey.as_deref(),
    )
    .await?;
    identity::save(&vault, &identity)?;
    vault.add_group_member(group_label, &peer_onion)?;
    println!(
        "[balchat] '{peer_onion}' añadido al grupo '{group_label}' (epoch ahora {})",
        group.epoch().as_u64()
    );

    if existing_members.is_empty() {
        return Ok(());
    }

    let commit_bytes = commit
        .tls_serialize_detached()
        .map_err(|e| anyhow!("serializar commit: {e:?}"))?;
    println!(
        "[balchat] diseminando Commit a {} miembro(s) existente(s)...",
        existing_members.len()
    );
    for member_onion in &existing_members {
        match deliver_to_member(
            &endpoint,
            &vault,
            &our_onion,
            member_onion,
            &group_record.mls_group_id,
            commit_bytes.clone(),
        )
        .await
        {
            Ok(via) => println!("    {member_onion} ← OK ({via})"),
            Err(e) => eprintln!("    {member_onion} ← FAIL: {e:#}"),
        }
    }
    Ok(())
}

async fn run_send_group(vault_path: &Path, group_label: &str, text: &str) -> Result<()> {
    let (vault, identity, base_dir) = open_vault_and_identity(vault_path)?;
    let group_record = vault
        .get_group_by_label(group_label)?
        .ok_or_else(|| anyhow!("grupo '{group_label}' no existe"))?;
    let members = vault.list_group_members(group_label)?;
    if members.is_empty() {
        bail!("grupo '{group_label}' no tiene otros miembros — invita primero");
    }

    let mut group = identity::load_group(&identity, &group_record.mls_group_id)?;
    let payload = AppPayload::Text(text.to_string());
    let mut payload_bytes = Vec::with_capacity(256);
    ciborium::ser::into_writer(&payload, &mut payload_bytes)
        .map_err(|e| anyhow!("CBOR encode: {e}"))?;
    let mls_out = group
        .create_message(&identity.provider, &identity.signer, &payload_bytes)
        .context("create_message para grupo")?;
    let blob = mls_out
        .tls_serialize_detached()
        .map_err(|e| anyhow!("serializar mls msg: {e:?}"))?;
    identity::save(&vault, &identity)?;

    println!("[balchat] bootstrap Arti...");
    let endpoint = Endpoint::bootstrap_in(&base_dir).await?;
    let our_onion = vault
        .kv_get(VAULT_KEY_MY_ONION)?
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();

    println!(
        "[balchat] enviando a {} miembro(s) del grupo '{group_label}'...",
        members.len()
    );
    for member_onion in &members {
        match deliver_to_member(
            &endpoint,
            &vault,
            &our_onion,
            member_onion,
            &group_record.mls_group_id,
            blob.clone(),
        )
        .await
        {
            Ok(via) => println!("    {member_onion} ← OK ({via})"),
            Err(e) => eprintln!("    {member_onion} ← FAIL: {e:#}"),
        }
    }
    Ok(())
}

/// Publica `count` KeyPackages frescos en tu relay para que otros peers puedan
/// iniciar handshake contigo aunque estés offline. Cada KP se consume con un solo
/// uso por la otra parte; mantener un pool de ~10 cubre invitaciones esporádicas.
async fn publish_kp_pool(
    endpoint: &Endpoint,
    vault: &Vault,
    identity: &Identity,
    count: u32,
) -> Result<u32> {
    let my_relay = identity::get_my_relay(vault)?
        .ok_or_else(|| anyhow!("no my-relay configurado (set-my-relay primero)"))?;
    let my_queue = identity::load_or_create_queue_id(vault)?;
    let client = RelayClient::new(endpoint);

    let mut last_pool_size = 0u32;
    for _ in 0..count {
        let kp_bundle = identity.fresh_key_package()?;
        let kp_bytes = kp_bundle
            .key_package()
            .tls_serialize_detached()
            .map_err(|e| anyhow!("serializar KeyPackage: {e:?}"))?;
        last_pool_size = client.put_keypackage(&my_relay, &my_queue, kp_bytes).await?;
    }
    identity::save(vault, identity)?;
    Ok(last_pool_size)
}

async fn run_publish_kp(vault_path: &Path, count: u32) -> Result<()> {
    let (vault, identity, base_dir) = open_vault_and_identity(vault_path)?;
    println!("[balchat] bootstrap Arti...");
    let endpoint = Endpoint::bootstrap_in(&base_dir).await?;
    let pool = publish_kp_pool(&endpoint, &vault, &identity, count).await?;
    println!("[balchat] {count} KeyPackages publicados; pool ahora = {pool}");
    Ok(())
}

async fn run_bootstrap_1to1(vault_path: &Path, target: &str) -> Result<()> {
    let (vault, identity, base_dir) = open_vault_and_identity(vault_path)?;
    let peer_onion = resolve_target_to_onion(&vault, target)?;
    let contact = vault
        .get_contact_by_onion(&peer_onion)?
        .ok_or_else(|| anyhow!("contacto desconocido — `add-contact` primero"))?;
    if contact.mls_group_id.is_some() {
        bail!("ya hay grupo MLS con este peer; usá `connect` o `send`");
    }
    let peer_relay = contact
        .relay_onion
        .clone()
        .ok_or_else(|| anyhow!("contact sin --relay configurado"))?;
    let peer_queue = contact
        .relay_queue_id
        .clone()
        .ok_or_else(|| anyhow!("contact sin --queue configurado"))?;

    println!("[balchat] bootstrap Arti...");
    let endpoint = Endpoint::bootstrap_in(&base_dir).await?;
    let client = RelayClient::new(&endpoint);

    println!("[balchat] consumiendo KeyPackage del relay del peer...");
    let kp_bytes = client
        .consume_keypackage(&peer_relay, &peer_queue)
        .await?
        .ok_or_else(|| {
            anyhow!(
                "pool de KeyPackages del peer vacío — pedile que corra \
                `balchat publish-kp` o `balchat watch --listen`"
            )
        })?;

    let peer_kp_in = openmls::prelude::KeyPackageIn::tls_deserialize_exact_bytes(&kp_bytes)
        .map_err(|e| anyhow!("deserializar KeyPackage del peer: {e:?}"))?
        .validate(
            openmls_traits::OpenMlsProvider::crypto(&identity.provider),
            openmls::prelude::ProtocolVersion::Mls10,
        )
        .map_err(|e| anyhow!("validar KeyPackage del peer: {e:?}"))?;

    if let Some(expected) = &contact.expected_pubkey {
        let actual = peer_kp_in.leaf_node().signature_key().as_slice();
        if actual != expected.as_slice() {
            bail!("pubkey del peer no coincide con --pubkey esperado en bootstrap offline");
        }
    }

    println!("[balchat] creando grupo MLS y Welcome...");
    let mut group = identity::create_group(&identity)?;
    let (_commit, welcome_out, _info) = group
        .add_members(&identity.provider, &identity.signer, &[peer_kp_in])
        .map_err(|e| anyhow!("add_members: {e:?}"))?;
    group
        .merge_pending_commit(&identity.provider)
        .map_err(|e| anyhow!("merge_pending_commit: {e:?}"))?;

    let welcome_bytes = welcome_out
        .tls_serialize_detached()
        .map_err(|e| anyhow!("serializar Welcome: {e:?}"))?;

    println!("[balchat] PUT Welcome al relay del peer (queue normal)...");
    let seq = client.put(&peer_relay, &peer_queue, welcome_bytes).await?;

    let group_id = group.group_id().as_slice().to_vec();
    vault.upsert_contact(&Contact {
        label: contact.label.clone(),
        onion_address: peer_onion.clone(),
        mls_group_id: Some(group_id.clone()),
        ..Default::default()
    })?;
    identity::save(&vault, &identity)?;

    println!(
        "[balchat] bootstrap OK — grupo {} establecido, Welcome encolado seq={seq}.",
        hex_short(&group_id)
    );
    println!("[balchat] cuando '{target}' haga `poll`/`watch`, joineará automáticamente.");
    Ok(())
}

async fn deliver_to_member(
    endpoint: &Endpoint,
    vault: &Vault,
    our_onion: &str,
    member_onion: &str,
    group_id: &[u8],
    blob: Vec<u8>,
) -> Result<&'static str> {
    let dial_target = if member_onion.contains(':') {
        member_onion.to_string()
    } else {
        format!("{member_onion}:{VIRTUAL_PORT}")
    };
    let dial = tokio::time::timeout(Duration::from_secs(30), endpoint.dial(&dial_target)).await;
    if let Ok(Ok(stream)) = dial {
        push_message_to_group_member(stream, our_onion, group_id, blob).await?;
        return Ok("directo");
    }
    let contact = vault
        .get_contact_by_onion(member_onion)?
        .ok_or_else(|| anyhow!("miembro {member_onion} no es un contacto y no responde directo"))?;
    let relay = contact
        .relay_onion
        .ok_or_else(|| anyhow!("miembro {member_onion} sin relay configurado y offline"))?;
    let queue = contact
        .relay_queue_id
        .ok_or_else(|| anyhow!("miembro {member_onion} sin queue_id configurado"))?;
    let client = RelayClient::new(endpoint);
    let _seq = client.put(&relay, &queue, blob).await?;
    Ok("relay")
}

// ---------- helpers ----------

fn resolve_target_to_onion(vault: &Vault, target: &str) -> Result<String> {
    if target.contains(".onion") {
        return Ok(normalize_onion(target));
    }
    for c in vault.list_contacts()? {
        if c.label == target {
            return Ok(c.onion_address);
        }
    }
    bail!("no encontré contacto con label '{target}'")
}

struct VaultResolver<'a> {
    vault: &'a Vault,
}
impl<'a> VaultResolver<'a> {
    fn new(vault: &'a Vault) -> Self {
        Self { vault }
    }
}
impl<'a> ResumeResolver for VaultResolver<'a> {
    fn group_id_for(&self, peer_onion: &str) -> Option<Vec<u8>> {
        match self.vault.get_contact_by_onion(peer_onion) {
            Ok(Some(c)) => c.mls_group_id,
            _ => None,
        }
    }
    fn knows_group_id(&self, group_id: &[u8]) -> bool {
        if matches!(self.vault.get_group_by_mls_id(group_id), Ok(Some(_))) {
            return true;
        }
        if let Ok(contacts) = self.vault.list_contacts() {
            return contacts
                .iter()
                .any(|c| c.mls_group_id.as_deref() == Some(group_id));
        }
        false
    }
    fn expected_pubkey_for(&self, peer_onion: &str) -> Option<Vec<u8>> {
        match self.vault.get_contact_by_onion(peer_onion) {
            Ok(Some(c)) => c.expected_pubkey,
            _ => None,
        }
    }
}

fn handle_outcome(vault: &Vault, outcome: &HandshakeOutcome) -> Result<()> {
    if let HandshakeOutcome::Fresh {
        group_id,
        peer_onion,
    } = outcome
    {
        let onion = normalize_onion(peer_onion);
        // Si ya existía contacto con ese onion, conservamos label y relay/queue;
        // upsert hace COALESCE.
        let existing = vault.get_contact_by_onion(&onion)?;
        let label = existing
            .as_ref()
            .map(|c| c.label.clone())
            .unwrap_or_else(|| peer_onion_label(&onion));
        vault.upsert_contact(&Contact {
            label,
            onion_address: onion,
            mls_group_id: Some(group_id.clone()),
            ..Default::default()
        })?;
    }
    Ok(())
}

fn outcome_summary(outcome: &HandshakeOutcome) -> &'static str {
    match outcome {
        HandshakeOutcome::Fresh { .. } => "fresh",
        HandshakeOutcome::Resumed { .. } => "resume",
    }
}

fn peer_onion_label(onion: &str) -> String {
    let head: String = onion.chars().take(8).collect();
    format!("peer-{head}")
}

fn normalize_onion(s: &str) -> String {
    if s.contains(':') {
        s.to_string()
    } else {
        format!("{s}:{VIRTUAL_PORT}")
    }
}

fn open_vault_and_identity(vault_path: &Path) -> Result<(Vault, Identity, PathBuf)> {
    if !vault_path.exists() {
        bail!(
            "vault {} no existe — `balchat init` primero",
            vault_path.display()
        );
    }
    let passphrase = read_passphrase("Passphrase: ", false)?;
    let vault = Vault::open(vault_path, &passphrase).context("abrir vault")?;
    let identity = identity::load_or_create(&vault, "me").context("cargar identidad")?;
    let base_dir = vault_path
        .parent()
        .ok_or_else(|| anyhow!("vault sin directorio padre"))?
        .to_path_buf();
    Ok((vault, identity, base_dir))
}

async fn retry_dial(ep: &Endpoint, target: &str, attempts: u32) -> Result<DataStream> {
    let mut last = None;
    for i in 1..=attempts {
        match ep.dial(target).await {
            Ok(s) => return Ok(s),
            Err(e) => {
                eprintln!("    intento {i}/{attempts}: {e:#}");
                last = Some(e);
                tokio::time::sleep(Duration::from_secs(15)).await;
            }
        }
    }
    Err(anyhow!(
        "no se pudo conectar a {target} tras {attempts} intentos: {:?}",
        last
    ))
}

async fn chat_repl(mut conv: Conversation, identity: &Identity, vault: &Vault) -> Result<()> {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    loop {
        tokio::select! {
            line = lines.next_line() => {
                match line.context("leer stdin")? {
                    Some(text) => {
                        if text.is_empty() { continue; }
                        conv.send_text(identity, &text).await?;
                        identity::save(vault, identity).ok();
                    }
                    None => {
                        println!("\n[balchat] EOF stdin, cerrando.");
                        return Ok(());
                    }
                }
            }
            recv = conv.recv_text(identity) => {
                match recv? {
                    Some(text) => {
                        println!("< {text}");
                        identity::save(vault, identity).ok();
                    }
                    None => {
                        println!("[balchat] peer cerró conexión.");
                        return Ok(());
                    }
                }
            }
        }
    }
}

fn read_passphrase(prompt: &str, confirm: bool) -> Result<String> {
    if let Ok(p) = std::env::var("BALCHAT_PASSPHRASE") {
        return Ok(p);
    }
    let p1 = rpassword::prompt_password(prompt).context("leer passphrase")?;
    if p1.is_empty() {
        bail!("passphrase vacía");
    }
    if confirm {
        let p2 = rpassword::prompt_password("Repite: ").context("repetir passphrase")?;
        if p1 != p2 {
            bail!("las passphrases no coinciden");
        }
    }
    Ok(p1)
}

fn expand_tilde(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(p)
}

fn hex_short(bytes: &[u8]) -> String {
    let n = bytes.len().min(6);
    let head: String = bytes[..n].iter().map(|b| format!("{b:02x}")).collect();
    format!("{head}…")
}

fn hex_short_owned(bytes: &[u8]) -> String {
    hex_short(bytes)
}
