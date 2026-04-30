//! balchat-relay — onion service no-confiable que almacena blobs cifrados por queue.
//!
//! Modelo:
//!   * Por cada conexión: 1 request → 1 response → close.
//!   * Storage: SQLite local (sin cifrado adicional — los blobs ya son ciphertext MLS).
//!   * Sin auth: cualquiera con un queue_id puede leer ese queue. Los blobs son
//!     ciphertext, así que no revelan contenido. Spam control queda para v2.

use anyhow::{Context, Result};
use balchat_core::{DataStream, Endpoint};
use balchat_relay_proto::{
    recv_frame, send_frame, QueueMessage, RelayRequest, RelayResponse, PROTOCOL_VERSION,
    QUEUE_ID_LEN,
};
use clap::Parser;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_NICKNAME: &str = "balchat-relay";

#[derive(Parser)]
#[command(name = "balchat-relay", about = "Relay no-confiable para mensajes balchat offline")]
struct Cli {
    /// Directorio donde se guardan messages.db y arti-state.
    #[arg(long, default_value = "~/.balchat-relay")]
    data_dir: String,
    /// Nickname interno del onion (gestiona claves).
    #[arg(long, default_value = DEFAULT_NICKNAME)]
    nickname: String,
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
    let data_dir = expand_tilde(&cli.data_dir);
    create_dir_secure(&data_dir)?;
    println!("[relay] data dir: {}", data_dir.display());

    let store = MessageStore::open(&data_dir.join("messages.db"))?;
    println!("[relay] storage abierto");

    println!("[relay] bootstrap Arti...");
    let endpoint = Endpoint::bootstrap_in(&data_dir).await?;

    println!("[relay] levantando onion '{}'...", cli.nickname);
    let mut handle = endpoint.host_onion(&cli.nickname).await?;
    println!("[relay] onion: {}", handle.onion);
    println!("[relay] aceptando conexiones (Ctrl+C para parar)");

    while let Some(stream) = handle.incoming.recv().await {
        let store = store.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_stream(stream, store).await {
                tracing::warn!("error en conexión: {:#}", e);
            }
        });
    }
    Ok(())
}

async fn handle_stream(mut stream: DataStream, store: MessageStore) -> Result<()> {
    let req: RelayRequest = recv_frame(&mut stream).await.context("recv RelayRequest")?;
    let resp = handle_request(&store, req);
    send_frame(&mut stream, &resp).await.context("send RelayResponse")?;
    Ok(())
}

fn handle_request(store: &MessageStore, req: RelayRequest) -> RelayResponse {
    match req {
        RelayRequest::Put {
            protocol_version,
            queue_id,
            blob,
        } => {
            if let Err(e) = check_proto_and_qid(protocol_version, &queue_id) {
                return e;
            }
            match store.put(&queue_id, &blob) {
                Ok(seq) => {
                    tracing::info!(
                        "PUT queue={} seq={seq} blob_bytes={}",
                        short_hex(&queue_id),
                        blob.len()
                    );
                    RelayResponse::PutAck { seq }
                }
                Err(e) => RelayResponse::Error {
                    msg: format!("put falló: {e:#}"),
                },
            }
        }
        RelayRequest::Get {
            protocol_version,
            queue_id,
            since_seq,
            max_messages,
        } => {
            if let Err(e) = check_proto_and_qid(protocol_version, &queue_id) {
                return e;
            }
            let limit = max_messages.clamp(1, 1024) as usize;
            match store.get(&queue_id, since_seq, limit) {
                Ok(messages) => {
                    tracing::info!(
                        "GET queue={} since={since_seq} returned={}",
                        short_hex(&queue_id),
                        messages.len()
                    );
                    RelayResponse::GetReply { messages }
                }
                Err(e) => RelayResponse::Error {
                    msg: format!("get falló: {e:#}"),
                },
            }
        }
        RelayRequest::PutKeyPackage {
            protocol_version,
            queue_id,
            key_package,
        } => {
            if let Err(e) = check_proto_and_qid(protocol_version, &queue_id) {
                return e;
            }
            match store.put_key_package(&queue_id, &key_package) {
                Ok(pool_size) => {
                    tracing::info!(
                        "PUT_KP queue={} pool_size={pool_size} kp_bytes={}",
                        short_hex(&queue_id),
                        key_package.len()
                    );
                    RelayResponse::PutKeyPackageAck { pool_size }
                }
                Err(e) => RelayResponse::Error {
                    msg: format!("put_kp falló: {e:#}"),
                },
            }
        }
        RelayRequest::ConsumeKeyPackage {
            protocol_version,
            queue_id,
        } => {
            if let Err(e) = check_proto_and_qid(protocol_version, &queue_id) {
                return e;
            }
            match store.consume_key_package(&queue_id) {
                Ok(opt) => {
                    tracing::info!(
                        "CONSUME_KP queue={} delivered={}",
                        short_hex(&queue_id),
                        opt.is_some()
                    );
                    RelayResponse::ConsumeKeyPackageReply { key_package: opt }
                }
                Err(e) => RelayResponse::Error {
                    msg: format!("consume_kp falló: {e:#}"),
                },
            }
        }
    }
}

fn check_proto_and_qid(version: u16, queue_id: &[u8]) -> std::result::Result<(), RelayResponse> {
    if version != PROTOCOL_VERSION {
        return Err(RelayResponse::Error {
            msg: format!("protocol_version {version} no soportada"),
        });
    }
    if queue_id.len() != QUEUE_ID_LEN {
        return Err(RelayResponse::Error {
            msg: format!("queue_id debe ser {QUEUE_ID_LEN} bytes"),
        });
    }
    Ok(())
}

#[derive(Clone)]
struct MessageStore {
    conn: Arc<Mutex<Connection>>,
}

impl MessageStore {
    fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("abrir {}", path.display()))?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS messages (
                queue_id   BLOB NOT NULL,
                seq        INTEGER NOT NULL,
                blob       BLOB NOT NULL,
                created_at INTEGER NOT NULL,
                PRIMARY KEY (queue_id, seq)
            ) WITHOUT ROWID;

            CREATE INDEX IF NOT EXISTS messages_qid_seq ON messages(queue_id, seq);

            CREATE TABLE IF NOT EXISTS key_packages (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                queue_id   BLOB NOT NULL,
                kp         BLOB NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS key_packages_qid ON key_packages(queue_id, id);

            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            "#,
        )
        .context("aplicar schema relay")?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn put(&self, queue_id: &[u8], blob: &[u8]) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let next_seq: i64 = conn.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM messages WHERE queue_id = ?1",
            params![queue_id],
            |r| r.get(0),
        )?;
        let now = unix_now();
        conn.execute(
            "INSERT INTO messages (queue_id, seq, blob, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![queue_id, next_seq, blob, now],
        )?;
        Ok(next_seq as u64)
    }

    fn get(&self, queue_id: &[u8], since: u64, limit: usize) -> Result<Vec<QueueMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT seq, blob FROM messages
             WHERE queue_id = ?1 AND seq > ?2
             ORDER BY seq LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(
                params![queue_id, since as i64, limit as i64],
                |r| {
                    let seq: i64 = r.get(0)?;
                    let blob: Vec<u8> = r.get(1)?;
                    Ok(QueueMessage {
                        seq: seq as u64,
                        blob,
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Añade un KeyPackage al pool del owner de `queue_id`. Devuelve el tamaño del pool.
    fn put_key_package(&self, queue_id: &[u8], kp: &[u8]) -> Result<u32> {
        let conn = self.conn.lock().unwrap();
        let now = unix_now();
        conn.execute(
            "INSERT INTO key_packages (queue_id, kp, created_at) VALUES (?1, ?2, ?3)",
            params![queue_id, kp, now],
        )?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM key_packages WHERE queue_id = ?1",
            params![queue_id],
            |r| r.get(0),
        )?;
        Ok(count as u32)
    }

    /// Consume el KeyPackage más antiguo del pool (FIFO). Lo BORRA del relay.
    /// Retorna `None` si el pool está vacío.
    fn consume_key_package(&self, queue_id: &[u8]) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().unwrap();
        let row: Option<(i64, Vec<u8>)> = conn
            .query_row(
                "SELECT id, kp FROM key_packages WHERE queue_id = ?1 ORDER BY id LIMIT 1",
                params![queue_id],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)),
            )
            .ok();
        let Some((id, kp)) = row else {
            return Ok(None);
        };
        conn.execute("DELETE FROM key_packages WHERE id = ?1", params![id])?;
        Ok(Some(kp))
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn short_hex(bytes: &[u8]) -> String {
    let n = bytes.len().min(4);
    bytes[..n].iter().map(|b| format!("{b:02x}")).collect()
}

fn expand_tilde(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(p)
}

fn create_dir_secure(p: &Path) -> Result<()> {
    std::fs::create_dir_all(p)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
