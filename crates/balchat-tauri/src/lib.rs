//! balchat desktop — frontend Tauri 2 sobre balchat-core.
//!
//! Modelo:
//!   * `unlock_vault(passphrase)` abre el SQLCipher y carga Identity en `AppState`.
//!   * `start_daemon` arranca un task background que hace bootstrap Arti, levanta el
//!     onion service, polea el relay, y procesa conexiones entrantes — emitiendo
//!     eventos `balchat://message` y `balchat://status` al frontend Svelte.
//!   * `send_text` corre el flow del CLI `send`: directo > relay fallback.

use anyhow::{anyhow, Context, Result};
use balchat_core::conversation::{
    AppPayload, Conversation, HandshakeOutcome, ResumeResolver, Role,
};
use balchat_core::relay_client::RelayClient;
use balchat_core::{identity, DataStream, Endpoint, HostHandle, Identity};
use balchat_storage::{Contact, Vault};
use openmls::prelude::tls_codec::Serialize as _;
use openmls::prelude::*;
use openmls_traits::OpenMlsProvider;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::Mutex;

const VIRTUAL_PORT: u16 = 1234;
const NICKNAME: &str = "balchat";
const VAULT_KEY_MY_ONION: &str = "my_onion.v1";

// -------- AppState compartido --------

struct AppState {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    vault_path: PathBuf,
    base_dir: PathBuf,
    vault: Option<Arc<Vault>>,
    identity: Option<Arc<Identity>>,
    endpoint: Option<Arc<Endpoint>>,
    daemon_running: bool,
}

impl AppState {
    fn new() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let base_dir = home.join(".balchat");
        let vault_path = base_dir.join("vault.db");
        Self {
            inner: Arc::new(Mutex::new(Inner {
                vault_path,
                base_dir,
                ..Default::default()
            })),
        }
    }
}

// -------- DTOs --------

#[derive(Serialize, Clone)]
struct MyId {
    onion: String,
    queue: String,
    relay: String,
}

#[derive(Serialize, Clone)]
struct ContactDto {
    label: String,
    onion_address: String,
    has_group: bool,
    has_relay: bool,
}

#[derive(Serialize, Clone)]
#[serde(tag = "kind")]
enum LogEntry {
    #[serde(rename = "received")]
    Received {
        from: String,
        from_label: Option<String>,
        text: String,
    },
    #[serde(rename = "info")]
    Info { text: String },
    #[serde(rename = "error")]
    Error { text: String },
}

#[derive(Serialize, Clone)]
struct StatusUpdate {
    status: &'static str,
}

/// Una entrada del histórico (proyección sobre `StoredMessage` para serializar
/// al frontend). El campo `created_at` es Unix timestamp en segundos.
#[derive(Serialize, Clone)]
struct MessageDto {
    direction: String,
    kind: String,
    body: String,
    created_at: i64,
}

// -------- Comandos Tauri --------

/// Indica si el vault ya existe en el path de la app. Útil para que el frontend
/// muestre "abrir" vs "crear nuevo" sin tener que probar y leer un error.
#[tauri::command]
async fn vault_exists(state: State<'_, AppState>) -> Result<bool, String> {
    let inner = state.inner.lock().await;
    Ok(inner.vault_path.exists())
}

/// Crea un vault nuevo con `passphrase` y `label`. Falla si ya existe uno.
/// Tras crear, el vault queda abierto en el AppState (mismo efecto que un
/// `unlock_vault` post-init).
#[tauri::command]
async fn create_vault(
    passphrase: String,
    label: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<MyId, String> {
    let mut inner = state.inner.lock().await;
    if inner.vault_path.exists() {
        return Err(format!(
            "ya existe un vault en {} — usá unlock en su lugar",
            inner.vault_path.display()
        ));
    }
    if let Some(parent) = inner.vault_path.parent() {
        std::fs::create_dir_all(parent).map_err(stringify)?;
    }
    let vault = Vault::open(&inner.vault_path, &passphrase).map_err(stringify)?;
    let label = if label.trim().is_empty() {
        "me".to_string()
    } else {
        label.trim().to_string()
    };
    let id = identity::load_or_create(&vault, &label).map_err(stringify)?;
    let queue = identity::load_or_create_queue_id(&vault).map_err(stringify)?;
    let relay = identity::get_my_relay(&vault).map_err(stringify)?.unwrap_or_default();
    let onion = vault
        .kv_get(VAULT_KEY_MY_ONION)
        .map_err(stringify)?
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();

    let dto = MyId {
        onion,
        queue: hex_encode(&queue),
        relay,
    };
    inner.vault = Some(Arc::new(vault));
    inner.identity = Some(Arc::new(id));
    drop(inner);
    // Auto-arranque del daemon: en mobile especialmente, obligar al usuario a
    // tocar "Arrancar daemon" después de cada unlock rompe el flujo natural.
    spawn_daemon_if_idle(state.inner.clone(), app);
    Ok(dto)
}

#[tauri::command]
async fn unlock_vault(
    passphrase: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<MyId, String> {
    let mut inner = state.inner.lock().await;
    if !inner.vault_path.exists() {
        return Err(format!(
            "vault no existe en {} — creá uno nuevo desde el botón \"Crear vault\".",
            inner.vault_path.display()
        ));
    }
    let vault = Vault::open(&inner.vault_path, &passphrase).map_err(stringify)?;
    let id = identity::load_or_create(&vault, "me").map_err(stringify)?;
    let queue = identity::load_or_create_queue_id(&vault).map_err(stringify)?;
    let relay = identity::get_my_relay(&vault).map_err(stringify)?.unwrap_or_default();
    let onion = vault
        .kv_get(VAULT_KEY_MY_ONION)
        .map_err(stringify)?
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();

    let dto = MyId {
        onion,
        queue: hex_encode(&queue),
        relay,
    };
    inner.vault = Some(Arc::new(vault));
    inner.identity = Some(Arc::new(id));
    drop(inner);
    spawn_daemon_if_idle(state.inner.clone(), app);
    Ok(dto)
}

/// Borra del estado de la app la referencia al vault y la identity. La UI
/// vuelve a la pantalla "Abrir vault" y los siguientes comandos que requieren
/// vault (send_text, list_contacts, etc.) fallan con "vault no abierto" hasta
/// el próximo `unlock_vault`.
///
/// El daemon en background sigue corriendo con sus propias `Arc<Vault>` y
/// `Arc<Identity>` clonadas — eso permite que mensajes entrantes se sigan
/// persistiendo y procesando hasta que se cierre la app, sin filtrar el log al
/// frontend lockeado (la UI ya no muestra `selected`/`log` mientras esté
/// `unlocked === false`). Para un lock "duro" que mate el daemon habría que
/// añadir cancel tokens, pero ese trade-off no compensa la complejidad si el
/// usuario sólo quiere proteger la pantalla.
#[tauri::command]
async fn lock_vault(state: State<'_, AppState>) -> Result<(), String> {
    let mut inner = state.inner.lock().await;
    inner.vault = None;
    inner.identity = None;
    Ok(())
}

#[tauri::command]
async fn list_contacts(state: State<'_, AppState>) -> Result<Vec<ContactDto>, String> {
    let inner = state.inner.lock().await;
    let vault = inner.vault.as_ref().ok_or("vault no abierto")?;
    let contacts = vault.list_contacts().map_err(stringify)?;
    Ok(contacts
        .into_iter()
        .map(|c| ContactDto {
            label: c.label,
            onion_address: c.onion_address,
            has_group: c.mls_group_id.is_some(),
            has_relay: c.relay_onion.is_some() && c.relay_queue_id.is_some(),
        })
        .collect())
}

/// Persiste un contacto en el vault. Equivalente al CLI `balchat add-contact`.
///
/// Campos:
///   * `label` — nombre amigable, no vacío.
///   * `onion` — `xxx.onion` o `xxx.onion:1234`. Se normaliza al puerto virtual.
///   * `relay`, `queue_hex`, `pubkey_hex` — opcionales (igual que en CLI).
///     `queue_hex` debe ser 64 chars (32 bytes); `pubkey_hex` 1..=128 bytes
///     después de decodear (mismas validaciones que `add-contact` del CLI).
///
/// Si ya existía un contacto con ese onion, se hace upsert: el `label` y los
/// campos provistos se actualizan, los no provistos se preservan
/// (`COALESCE(excluded, contacts)` en el SQL).
#[tauri::command]
async fn add_contact_cmd(
    label: String,
    onion: String,
    relay: Option<String>,
    queue_hex: Option<String>,
    pubkey_hex: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let label = label.trim().to_string();
    if label.is_empty() {
        return Err("label no puede estar vacío".into());
    }
    let onion = onion.trim().to_string();
    if onion.is_empty() {
        return Err("onion no puede estar vacío".into());
    }
    let normalized = if onion.contains(':') {
        onion.clone()
    } else {
        format!("{onion}:{VIRTUAL_PORT}")
    };
    let queue_id = match queue_hex.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => Some(hex::decode(s).map_err(|e| format!("queue debe ser hex: {e}"))?),
        None => None,
    };
    if let Some(q) = &queue_id {
        if q.len() != 32 {
            return Err(format!(
                "queue_id debe ser 32 bytes (64 hex chars), recibido {} bytes",
                q.len()
            ));
        }
    }
    let expected_pubkey = match pubkey_hex.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => Some(hex::decode(s).map_err(|e| format!("pubkey debe ser hex: {e}"))?),
        None => None,
    };
    if let Some(p) = &expected_pubkey {
        if p.is_empty() || p.len() > 128 {
            return Err(format!("pubkey de longitud inesperada: {} bytes", p.len()));
        }
    }
    let relay_onion = relay
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);

    let inner = state.inner.lock().await;
    let vault = inner.vault.as_ref().ok_or("vault no abierto")?;
    vault
        .upsert_contact(&Contact {
            label,
            onion_address: normalized,
            relay_onion,
            relay_queue_id: queue_id,
            expected_pubkey,
            ..Default::default()
        })
        .map_err(stringify)?;
    Ok(())
}

/// Borra un contacto del vault y todos sus mensajes asociados. `peer` se
/// normaliza al puerto virtual igual que en los demás comandos.
#[tauri::command]
async fn delete_contact_cmd(
    peer: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let inner = state.inner.lock().await;
    let vault = inner.vault.as_ref().ok_or("vault no abierto")?;
    let normalized = if peer.contains(':') {
        peer
    } else {
        format!("{peer}:{VIRTUAL_PORT}")
    };
    vault
        .delete_contact_and_messages(&normalized)
        .map_err(stringify)?;
    Ok(())
}

/// Devuelve los últimos `limit` mensajes guardados en el vault para un peer
/// (`peer` puede o no incluir `:1234` — se normaliza igual que los demás).
/// Si `limit == 0`, retorna todo el histórico (orden cronológico).
#[tauri::command]
async fn list_messages_cmd(
    peer: String,
    limit: u32,
    state: State<'_, AppState>,
) -> Result<Vec<MessageDto>, String> {
    let inner = state.inner.lock().await;
    let vault = inner.vault.as_ref().ok_or("vault no abierto")?;
    let normalized = if peer.contains(':') {
        peer
    } else {
        format!("{peer}:{VIRTUAL_PORT}")
    };
    let rows = vault.list_messages(&normalized, limit).map_err(stringify)?;
    Ok(rows
        .into_iter()
        .map(|m| MessageDto {
            direction: m.direction,
            kind: m.kind,
            body: m.body,
            created_at: m.created_at,
        })
        .collect())
}

#[tauri::command]
async fn start_daemon(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    {
        let inner = state.inner.lock().await;
        if inner.vault.is_none() {
            return Err("vault no abierto".into());
        }
        if inner.daemon_running {
            return Err("daemon ya está corriendo".into());
        }
    }
    spawn_daemon_if_idle(state.inner.clone(), app);
    Ok(())
}

/// Arranca el daemon en un task de tokio si no está ya corriendo. Idempotente:
/// si `daemon_running` ya es `true`, no hace nada (no es un error). Pensado para
/// el camino auto-start de `unlock_vault`/`create_vault`, donde no queremos que
/// un re-unlock falle si el daemon ya quedó arriba de la sesión anterior.
fn spawn_daemon_if_idle(inner_arc: Arc<Mutex<Inner>>, app: AppHandle) {
    let app_for_task = app.clone();
    tokio::spawn(async move {
        {
            let inner = inner_arc.lock().await;
            if inner.daemon_running {
                tracing::debug!("daemon ya corre; spawn_daemon_if_idle no-op");
                return;
            }
        }
        emit_status(&app_for_task, "starting");
        if let Err(e) = run_daemon(inner_arc.clone(), app_for_task.clone()).await {
            tracing::error!("daemon crashed: {e:#}");
            let _ = app_for_task.emit("balchat://status", StatusUpdate { status: "error" });
            let _ = app_for_task.emit(
                "balchat://message",
                LogEntry::Error {
                    text: format!("daemon error: {e:#}"),
                },
            );
            // Permitir un retry futuro (ej: el botón "Arrancar daemon"): marcamos
            // explícitamente como no-corriendo aunque run_daemon nunca llegó a
            // setearlo `true` (boot Arti pudo fallar antes).
            let mut inner = inner_arc.lock().await;
            inner.daemon_running = false;
        }
    });
}

#[tauri::command]
async fn send_text(
    peer: String,
    text: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let (vault, identity, endpoint) = {
        let inner = state.inner.lock().await;
        let v = inner.vault.as_ref().ok_or("vault no abierto")?.clone();
        let i = inner.identity.as_ref().ok_or("identidad no cargada")?.clone();
        let e = inner
            .endpoint
            .as_ref()
            .ok_or("daemon no arrancado: 'Arrancar daemon' primero")?
            .clone();
        (v, i, e)
    };

    let normalized = if peer.contains(':') {
        peer.clone()
    } else {
        format!("{peer}:{VIRTUAL_PORT}")
    };
    let contact = vault
        .get_contact_by_onion(&normalized)
        .map_err(stringify)?
        .ok_or("contacto desconocido")?;
    if contact.mls_group_id.is_none() {
        return Err(
            "no hay grupo MLS — primero hacé handshake live (CLI: balchat connect)".into(),
        );
    }

    let payload = AppPayload::Text(text.clone());
    send_with_fallback(&endpoint, &vault, &identity, &normalized, &contact, &payload)
        .await
        .map_err(stringify)?;
    if let Err(e) = vault.insert_message(&normalized, "sent", "text", &text) {
        tracing::warn!("insert_message (sent text) falló: {e:#}");
    }
    Ok(())
}

/// Manda un archivo del filesystem al peer. El frontend abre el dialog de selección
/// (`tauri-plugin-dialog`) y pasa el path absoluto a este comando, que lee el archivo
/// y lo serializa como `AppPayload::File`.
#[tauri::command]
async fn send_file_path(
    peer: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let (vault, identity, endpoint) = {
        let inner = state.inner.lock().await;
        let v = inner.vault.as_ref().ok_or("vault no abierto")?.clone();
        let i = inner.identity.as_ref().ok_or("identidad no cargada")?.clone();
        let e = inner
            .endpoint
            .as_ref()
            .ok_or("daemon no arrancado: 'Arrancar daemon' primero")?
            .clone();
        (v, i, e)
    };

    let normalized = if peer.contains(':') {
        peer.clone()
    } else {
        format!("{peer}:{VIRTUAL_PORT}")
    };
    let contact = vault
        .get_contact_by_onion(&normalized)
        .map_err(stringify)?
        .ok_or("contacto desconocido")?;
    if contact.mls_group_id.is_none() {
        return Err(
            "no hay grupo MLS — primero hacé handshake live (CLI: balchat connect)".into(),
        );
    }

    let path_buf = std::path::PathBuf::from(&path);
    let filename = path_buf
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .ok_or("path sin filename")?;
    let data = std::fs::read(&path_buf)
        .map_err(|e| format!("leer {}: {e}", path_buf.display()))?;
    if data.len() > 14 * 1024 * 1024 {
        return Err(format!(
            "archivo demasiado grande ({} MB; límite ~14 MB para un solo MLS msg)",
            data.len() / 1024 / 1024
        ));
    }

    let payload = AppPayload::File {
        filename: filename.clone(),
        data,
    };
    send_with_fallback(&endpoint, &vault, &identity, &normalized, &contact, &payload)
        .await
        .map_err(stringify)?;
    if let Err(e) = vault.insert_message(&normalized, "sent", "file", &filename) {
        tracing::warn!("insert_message (sent file) falló: {e:#}");
    }
    Ok(())
}

// -------- Send con fallback (directo > relay) --------

async fn send_with_fallback(
    endpoint: &Endpoint,
    vault: &Vault,
    identity: &Identity,
    peer_onion: &str,
    contact: &Contact,
    payload: &AppPayload,
) -> Result<()> {
    let dial =
        tokio::time::timeout(Duration::from_secs(45), endpoint.dial(peer_onion)).await;
    if let Ok(Ok(stream)) = dial {
        return send_via_direct(stream, identity, vault, peer_onion, payload).await;
    }
    // fallback al relay
    let relay_onion = contact
        .relay_onion
        .clone()
        .ok_or_else(|| anyhow!("contact sin --relay y peer no responde directo"))?;
    let queue_id = contact
        .relay_queue_id
        .clone()
        .ok_or_else(|| anyhow!("contact sin --queue"))?;
    let group_id = contact
        .mls_group_id
        .clone()
        .ok_or_else(|| anyhow!("contact sin mls_group_id"))?;

    let mut group =
        MlsGroup::load(identity.provider.storage(), &GroupId::from_slice(&group_id))
            .map_err(|e| anyhow!("MlsGroup::load: {e:?}"))?
            .ok_or_else(|| anyhow!("group_id no en MLS storage"))?;
    let mut payload_bytes = Vec::with_capacity(256);
    ciborium::ser::into_writer(payload, &mut payload_bytes)
        .map_err(|e| anyhow!("CBOR encode: {e}"))?;
    let mls_out = group
        .create_message(&identity.provider, &identity.signer, &payload_bytes)
        .context("create_message")?;
    let blob = mls_out.tls_serialize_detached().context("serializar MLS")?;
    identity::save(vault, identity)?;

    let client = RelayClient::new(endpoint);
    let _seq = client.put(&relay_onion, &queue_id, blob).await?;
    Ok(())
}

async fn send_via_direct(
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
    let resolver = VaultResolver { vault };
    let (mut conv, outcome) = Conversation::open(
        stream,
        identity,
        Role::Initiator,
        &our_onion,
        Some(peer_onion),
        &resolver,
    )
    .await?;
    save_outcome(vault, &outcome)?;
    identity::save(vault, identity)?;
    conv.send_app(identity, payload).await?;
    identity::save(vault, identity)?;
    conv.say_goodbye().await?;
    Ok(())
}

// -------- Daemon --------

async fn run_daemon(inner_arc: Arc<Mutex<Inner>>, app: AppHandle) -> Result<()> {
    let base_dir = inner_arc.lock().await.base_dir.clone();
    let endpoint = Arc::new(Endpoint::bootstrap_in(&base_dir).await?);
    {
        let mut inner = inner_arc.lock().await;
        inner.endpoint = Some(endpoint.clone());
    }

    let mut handle: HostHandle = endpoint.host_onion(NICKNAME).await?;
    let our_onion = format!("{}:{}", handle.onion, VIRTUAL_PORT);
    let (vault, identity) = {
        let inner = inner_arc.lock().await;
        let v = inner.vault.as_ref().ok_or_else(|| anyhow!("vault gone"))?.clone();
        let i = inner.identity.as_ref().ok_or_else(|| anyhow!("id gone"))?.clone();
        (v, i)
    };
    vault.kv_set(VAULT_KEY_MY_ONION, our_onion.as_bytes())?;
    emit_status(&app, "running");
    let _ = app.emit(
        "balchat://message",
        LogEntry::Info {
            text: format!("listening: {our_onion}"),
        },
    );
    {
        let mut inner = inner_arc.lock().await;
        inner.daemon_running = true;
    }

    let interval = Duration::from_secs(30);
    let max_messages = 64u32;
    let client = RelayClient::new(&endpoint);
    let my_relay = identity::get_my_relay(&vault)?;
    let my_queue = identity::load_or_create_queue_id(&vault)?;

    if let Some(ref r) = my_relay {
        let _ = poll_and_emit(&client, r, &my_queue, &vault, &identity, &app, max_messages).await;
    }

    loop {
        tokio::select! {
            _ = tokio::time::sleep(interval) => {
                if let Some(ref r) = my_relay {
                    let _ = poll_and_emit(&client, r, &my_queue, &vault, &identity, &app, max_messages).await;
                }
            }
            stream = handle.incoming.recv() => {
                match stream {
                    Some(s) => {
                        if let Err(e) = handle_incoming(s, &vault, &identity, &app, &our_onion).await {
                            let _ = app.emit("balchat://message", LogEntry::Error {
                                text: format!("conn entrante falló: {e:#}"),
                            });
                        }
                    }
                    None => return Ok(()),
                }
            }
        }
    }
}

async fn poll_and_emit(
    client: &RelayClient<'_>,
    relay: &str,
    queue: &[u8],
    vault: &Vault,
    identity: &Identity,
    app: &AppHandle,
    max: u32,
) -> Result<()> {
    let last_seq = vault.get_last_seq(relay, queue)?;
    let messages = client.get(relay, queue, last_seq, max).await?;
    if messages.is_empty() {
        return Ok(());
    }

    let mut new_last = last_seq;
    for msg in &messages {
        // Detectar Welcomes que llegan vía relay (peer offline nos invitó a un
        // grupo). Si el blob es un Welcome, joineamos el grupo, lo registramos
        // en el vault con un label sintético para que aparezca en la lista de
        // contactos/grupos, y emitimos un Info al frontend. Después seguimos al
        // siguiente blob (no llamamos a decrypt_blob porque no es App message).
        if identity::blob_is_welcome(&msg.blob) {
            match identity::process_welcome_blob(identity, &msg.blob) {
                Ok(group_id) => {
                    if let Err(e) = identity::save(vault, identity) {
                        tracing::warn!("identity::save tras Welcome falló: {e:#}");
                    }
                    if vault
                        .get_group_by_mls_id(&group_id)
                        .ok()
                        .flatten()
                        .is_none()
                    {
                        let label = format!("inbox-{}", &hex_encode(&group_id)[..8]);
                        if let Err(e) = vault.create_group(&label, &group_id) {
                            tracing::warn!("registrar grupo offline en vault falló: {e:#}");
                        }
                    }
                    let _ = app.emit(
                        "balchat://message",
                        LogEntry::Info {
                            text: format!(
                                "joineado grupo MLS via Welcome offline (group_id={})",
                                &hex_encode(&group_id)[..16]
                            ),
                        },
                    );
                    send_system_notification(
                        app,
                        "balchat",
                        "te han invitado a un grupo nuevo",
                    );
                }
                Err(e) => {
                    let _ = app.emit(
                        "balchat://message",
                        LogEntry::Error {
                            text: format!("procesar Welcome falló (seq={}): {e:#}", msg.seq),
                        },
                    );
                }
            }
            if msg.seq > new_last {
                new_last = msg.seq;
            }
            continue;
        }

        match decrypt_blob(identity, &msg.blob) {
            Ok((payload, group_id)) => {
                // Resolvemos al contacto vía group_id; si no lo encontramos,
                // marcamos con un pseudo-id basado en el queue. La persistencia
                // usa el onion real cuando está disponible para que el histórico
                // se vea bajo el contacto correcto al seleccionarlo en la UI.
                let contact = contact_for_group_id(vault, &group_id);
                let (from_id, from_label) = match &contact {
                    Some(c) => (c.onion_address.clone(), Some(c.label.clone())),
                    None => (format!("relay:{}", &hex_encode(queue)[..8]), None),
                };
                match payload {
                    AppPayload::Text(t) => {
                        if let Err(e) = vault.insert_message(&from_id, "received", "text", &t) {
                            tracing::warn!("insert_message (relay text) falló: {e:#}");
                        }
                        let _ = app.emit(
                            "balchat://message",
                            LogEntry::Received {
                                from: from_id.clone(),
                                from_label: from_label.clone(),
                                text: t.clone(),
                            },
                        );
                        let display = from_label.unwrap_or_else(|| from_id.clone());
                        send_system_notification(app, &display, &t);
                    }
                    AppPayload::File { filename, data } => {
                        if let Err(e) =
                            vault.insert_message(&from_id, "received", "file", &filename)
                        {
                            tracing::warn!("insert_message (relay file) falló: {e:#}");
                        }
                        let _ = app.emit(
                            "balchat://message",
                            LogEntry::Info {
                                text: format!(
                                    "archivo recibido (relay): {filename} ({} bytes)",
                                    data.len()
                                ),
                            },
                        );
                        let display = from_label.unwrap_or_else(|| "balchat".to_string());
                        send_system_notification(
                            app,
                            &display,
                            &format!("archivo recibido: {filename} ({} bytes)", data.len()),
                        );
                    }
                }
            }
            Err(e) => {
                let _ = app.emit(
                    "balchat://message",
                    LogEntry::Error {
                        text: format!("descifrado falló (seq={}): {e:#}", msg.seq),
                    },
                );
            }
        }
        identity::save(vault, identity)?;
        if msg.seq > new_last {
            new_last = msg.seq;
        }
    }
    vault.set_last_seq(relay, queue, new_last)?;
    Ok(())
}

async fn handle_incoming(
    stream: DataStream,
    vault: &Vault,
    identity: &Identity,
    app: &AppHandle,
    our_onion: &str,
) -> Result<()> {
    let resolver = VaultResolver { vault };
    let (mut conv, outcome) = Conversation::open(
        stream,
        identity,
        Role::Acceptor,
        our_onion,
        None,
        &resolver,
    )
    .await?;
    save_outcome(vault, &outcome)?;
    identity::save(vault, identity)?;
    let from = conv.peer_onion.clone();
    let _ = app.emit(
        "balchat://message",
        LogEntry::Info {
            text: format!("conn entrante de {from}"),
        },
    );

    while let Some(payload) = conv.recv_app(identity).await? {
        match payload {
            AppPayload::Text(t) => {
                let label = vault
                    .get_contact_by_onion(&from)
                    .ok()
                    .flatten()
                    .map(|c| c.label);
                let display = label.clone().unwrap_or_else(|| from.clone());
                if let Err(e) = vault.insert_message(&from, "received", "text", &t) {
                    tracing::warn!("insert_message (live text) falló: {e:#}");
                }
                let _ = app.emit(
                    "balchat://message",
                    LogEntry::Received {
                        from: from.clone(),
                        from_label: label,
                        text: t.clone(),
                    },
                );
                send_system_notification(app, &display, &t);
            }
            AppPayload::File { filename, data } => {
                if let Err(e) = vault.insert_message(&from, "received", "file", &filename) {
                    tracing::warn!("insert_message (live file) falló: {e:#}");
                }
                let _ = app.emit(
                    "balchat://message",
                    LogEntry::Info {
                        text: format!("[de {from}] archivo {filename} ({} bytes)", data.len()),
                    },
                );
                send_system_notification(
                    app,
                    &from,
                    &format!("archivo {filename} ({} bytes)", data.len()),
                );
            }
        }
        identity::save(vault, identity)?;
    }
    Ok(())
}

/// Manda una notification del sistema operativo (Android: status bar / lock screen).
/// Silencia errores: si el plugin no está disponible o la permission no concedida,
/// solo loggeamos warning.
fn send_system_notification(app: &AppHandle, title: &str, body: &str) {
    let truncated = if body.len() > 200 {
        format!("{}…", &body[..200])
    } else {
        body.to_string()
    };
    let res = app
        .notification()
        .builder()
        .title(title)
        .body(&truncated)
        .show();
    if let Err(e) = res {
        tracing::warn!("notification.show falló: {e:#}");
    }
}

/// Descifra un blob MLS y devuelve además el `group_id` del MLS group, para que
/// el caller pueda resolverlo a un contacto (relevante en mensajes via relay,
/// donde el blob no lleva el onion del sender).
fn decrypt_blob(identity: &Identity, blob: &[u8]) -> Result<(AppPayload, Vec<u8>)> {
    let in_msg = MlsMessageIn::tls_deserialize_exact_bytes(blob).context("deserializar MLS")?;
    let proto: ProtocolMessage = in_msg
        .try_into_protocol_message()
        .map_err(|_| anyhow!("frame no es ProtocolMessage"))?;
    let group_id = proto.group_id().clone();
    let group_id_bytes = group_id.as_slice().to_vec();
    let mut group = MlsGroup::load(identity.provider.storage(), &group_id)
        .map_err(|e| anyhow!("MlsGroup::load: {e:?}"))?
        .ok_or_else(|| anyhow!("group_id no en mi storage"))?;
    let processed = group
        .process_message(&identity.provider, proto)
        .context("process_message")?;
    match processed.into_content() {
        ProcessedMessageContent::ApplicationMessage(app) => {
            let bytes = app.into_bytes();
            let payload = match ciborium::de::from_reader::<AppPayload, _>(&bytes[..]) {
                Ok(p) => p,
                Err(_) => AppPayload::Text(String::from_utf8_lossy(&bytes).into_owned()),
            };
            Ok((payload, group_id_bytes))
        }
        other => Err(anyhow!("contenido inesperado: {other:?}")),
    }
}

/// Busca un contacto por `mls_group_id`. Útil para resolver mensajes recibidos
/// vía relay (donde no llega el onion del peer) al contacto correcto del vault.
fn contact_for_group_id(vault: &Vault, group_id: &[u8]) -> Option<Contact> {
    vault
        .list_contacts()
        .ok()?
        .into_iter()
        .find(|c| c.mls_group_id.as_deref() == Some(group_id))
}

// -------- Helpers --------

fn save_outcome(vault: &Vault, outcome: &HandshakeOutcome) -> Result<()> {
    if let HandshakeOutcome::Fresh {
        group_id,
        peer_onion,
    } = outcome
    {
        let onion = if peer_onion.contains(':') {
            peer_onion.clone()
        } else {
            format!("{peer_onion}:{VIRTUAL_PORT}")
        };
        let existing = vault.get_contact_by_onion(&onion)?;
        let label = existing
            .as_ref()
            .map(|c| c.label.clone())
            .unwrap_or_else(|| short_label(&onion));
        vault.upsert_contact(&Contact {
            label,
            onion_address: onion,
            mls_group_id: Some(group_id.clone()),
            ..Default::default()
        })?;
    }
    Ok(())
}

fn short_label(onion: &str) -> String {
    format!("peer-{}", onion.chars().take(8).collect::<String>())
}

struct VaultResolver<'a> {
    vault: &'a Vault,
}
impl<'a> ResumeResolver for VaultResolver<'a> {
    fn group_id_for(&self, peer_onion: &str) -> Option<Vec<u8>> {
        match self.vault.get_contact_by_onion(peer_onion) {
            Ok(Some(c)) => c.mls_group_id,
            _ => None,
        }
    }
}

fn stringify<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn emit_status(app: &AppHandle, status: &'static str) {
    let _ = app.emit("balchat://status", StatusUpdate { status });
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// -------- entry points (desktop + mobile) --------

/// Punto de entrada compartido: desktop lo llama desde `fn main()`, Android desde
/// el `JNI_OnLoad` que genera Tauri vía `#[tauri::mobile_entry_point]`.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,arti=warn,tor_=warn")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            vault_exists,
            create_vault,
            unlock_vault,
            lock_vault,
            list_contacts,
            add_contact_cmd,
            delete_contact_cmd,
            list_messages_cmd,
            start_daemon,
            send_text,
            send_file_path,
        ])
        .setup(|app| {
            // En mobile, $HOME no existe — usamos el `app_local_data_dir` que da Tauri
            // (en Android es algo como /data/data/com.balchat.mobile/files/).
            // En desktop dejamos el default (`$HOME/.balchat/`) para no romper vaults.
            #[cfg(mobile)]
            {
                use tauri::Manager;
                if let Ok(dir) = app.path().app_local_data_dir() {
                    let _ = std::fs::create_dir_all(&dir);
                    let state: tauri::State<AppState> = app.state();
                    let inner_arc = state.inner.clone();
                    tauri::async_runtime::block_on(async move {
                        let mut inner = inner_arc.lock().await;
                        inner.base_dir = dir.clone();
                        inner.vault_path = dir.join("vault.db");
                    });
                }
            }
            #[cfg(not(mobile))]
            {
                let _ = app;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error corriendo tauri");
}
