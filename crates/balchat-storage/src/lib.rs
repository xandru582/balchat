//! balchat-storage — vault SQLCipher para persistir identidad, contactos y MLS state.
//!
//! Diseño:
//!   * Una sola base SQLite cifrada con SQLCipher (PRAGMA key).
//!   * Schema mínimo: tabla `kv` para singletons y `contacts` para peers.
//!   * No conoce nada de MLS ni Tor — solo blobs y metadatos. La capa `balchat-core`
//!     decide qué serializa/deserializa.

use anyhow::{anyhow, Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::RngCore;
use rusqlite::{params, Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

// Argon2id: 64 MB memory, 3 iter, 4 lanes, 32-byte output. Razonable para desktop;
// añade ~1s de latency al unlock pero hace brute force con GPU mucho más caro.
//
// En `cfg(test)` reducimos drásticamente `m_cost` y `t_cost` para que los tests
// paralelos no exploten memoria (4 × 64 MB simultáneos era demasiado para CI).
const ARGON2_SALT_LEN: usize = 16;
const ARGON2_KEY_LEN: usize = 32;

#[cfg(not(test))]
const ARGON2_M_COST: u32 = 65536;
#[cfg(not(test))]
const ARGON2_T_COST: u32 = 3;
#[cfg(not(test))]
const ARGON2_P_COST: u32 = 4;

#[cfg(test)]
const ARGON2_M_COST: u32 = 256;
#[cfg(test)]
const ARGON2_T_COST: u32 = 1;
#[cfg(test)]
const ARGON2_P_COST: u32 = 1;

/// Vault — handle a la base SQLCipher abierta y descifrada.
///
/// `Connection` no implementa `Sync`, así que la envolvemos en `Mutex` para que
/// `Vault` sí lo sea (necesario para usarlo dentro de `tauri::State<T>` y similares).
pub struct Vault {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl Vault {
    /// Abre o crea un vault. Si el archivo no existe, crea uno nuevo, genera un salt
    /// para Argon2id, deriva una llave de 32 bytes y la usa como `PRAGMA key`.
    ///
    /// Back-compat: si el vault existe pero NO tiene `<path>.salt`, asumimos modo
    /// legacy (passphrase pasada directo a SQLCipher). Vaults nuevos creados aquí
    /// siempre usan Argon2id.
    pub fn open(path: impl AsRef<Path>, passphrase: &str) -> Result<Self> {
        let path = path.as_ref().to_owned();
        let existed = path.exists();
        let salt_path = salt_path_for(&path);

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("crear directorio {}", parent.display()))?;
        }

        // Decidir modo y obtener salt si aplica.
        let argon2_salt: Option<Vec<u8>> = if existed {
            if salt_path.exists() {
                let s = std::fs::read(&salt_path)
                    .with_context(|| format!("leer salt {}", salt_path.display()))?;
                if s.len() != ARGON2_SALT_LEN {
                    return Err(anyhow!(
                        "salt file {} tiene tamaño inesperado: {} (esperaba {ARGON2_SALT_LEN})",
                        salt_path.display(),
                        s.len()
                    ));
                }
                Some(s)
            } else {
                None // legacy
            }
        } else {
            // Nuevo vault: siempre Argon2id.
            let mut s = vec![0u8; ARGON2_SALT_LEN];
            rand::thread_rng().fill_bytes(&mut s);
            Some(s)
        };

        let conn = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
        )
        .with_context(|| format!("abrir SQLite en {}", path.display()))?;

        // PRAGMA key DEBE ser lo primero. Aplicamos según modo:
        //  * Argon2id: derivamos clave y la pasamos como hex literal `x'...'` con
        //    `cipher_kdf_iter = 1` (saltamos PBKDF2 interno; Argon2 ya hizo el work).
        //  * Legacy: passphrase directa, SQLCipher hace su PBKDF2 default.
        if let Some(salt) = &argon2_salt {
            let key = derive_argon2_key(passphrase.as_bytes(), salt)
                .context("derivar clave Argon2id")?;
            let hex_key: String = key.iter().map(|b| format!("{b:02x}")).collect();
            conn.execute_batch(&format!(
                "PRAGMA key = \"x'{hex_key}'\";\n\
                 PRAGMA cipher_kdf_iter = 1;\n\
                 PRAGMA cipher_compatibility = 4;\n\
                 PRAGMA foreign_keys = ON;\n\
                 PRAGMA journal_mode = WAL;\n\
                 PRAGMA synchronous = NORMAL;\n\
                 PRAGMA secure_delete = ON;",
            ))
            .context("aplicar PRAGMAs (Argon2 key + hardening)")?;
        } else {
            conn.execute_batch(&format!(
                "PRAGMA key = {};\n\
                 PRAGMA cipher_compatibility = 4;\n\
                 PRAGMA foreign_keys = ON;\n\
                 PRAGMA journal_mode = WAL;\n\
                 PRAGMA synchronous = NORMAL;\n\
                 PRAGMA secure_delete = ON;",
                quote_sql_string(passphrase)
            ))
            .context("aplicar PRAGMAs (legacy key + hardening)")?;
        }

        // Probe: si la passphrase está mal, este SELECT falla con "file is not a database".
        let _: i64 = conn
            .query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0))
            .context("validar passphrase (lectura de sqlite_master)")?;

        if !existed {
            tracing::info!("creando schema en {}", path.display());
            conn.execute_batch(SCHEMA).context("aplicar schema")?;
            // Persistir salt SOLO tras éxito creando el schema, para no dejar un
            // salt huérfano si SQLite falló al inicializar.
            if let Some(salt) = &argon2_salt {
                std::fs::write(&salt_path, salt).with_context(|| {
                    format!("escribir salt {}", salt_path.display())
                })?;
            }
        } else {
            apply_migrations(&conn).context("aplicar migraciones")?;
        }

        Ok(Self {
            conn: Mutex::new(conn),
            path,
        })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("vault Mutex poisoned")
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    // ---------- KV singletons ----------

    pub fn kv_get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let row: Option<Vec<u8>> = self
            .conn()
            .query_row(
                "SELECT value FROM kv WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
            .with_context(|| format!("kv_get({key})"))?
            .map_or(None, Some);
        Ok(row)
    }

    pub fn kv_set(&self, key: &str, value: &[u8]) -> Result<()> {
        let now = unix_now();
        self.conn()
            .execute(
                "INSERT INTO kv (key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value,
                                                updated_at = excluded.updated_at",
                params![key, value, now],
            )
            .with_context(|| format!("kv_set({key})"))?;
        Ok(())
    }

    pub fn kv_delete(&self, key: &str) -> Result<()> {
        self.conn()
            .execute("DELETE FROM kv WHERE key = ?1", params![key])
            .with_context(|| format!("kv_delete({key})"))?;
        Ok(())
    }

    // ---------- Contacts ----------

    pub fn upsert_contact(&self, contact: &Contact) -> Result<i64> {
        let now = unix_now();
        let conn = self.conn();
        conn
            .execute(
                "INSERT INTO contacts (label, onion_address, mls_group_id, mls_group_state,
                                       relay_onion, relay_queue_id, expected_pubkey,
                                       created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
                 ON CONFLICT(onion_address) DO UPDATE SET
                     label = excluded.label,
                     mls_group_id = COALESCE(excluded.mls_group_id, contacts.mls_group_id),
                     mls_group_state = COALESCE(excluded.mls_group_state, contacts.mls_group_state),
                     relay_onion = COALESCE(excluded.relay_onion, contacts.relay_onion),
                     relay_queue_id = COALESCE(excluded.relay_queue_id, contacts.relay_queue_id),
                     expected_pubkey = COALESCE(excluded.expected_pubkey, contacts.expected_pubkey),
                     updated_at = excluded.updated_at",
                params![
                    contact.label,
                    contact.onion_address,
                    contact.mls_group_id,
                    contact.mls_group_state,
                    contact.relay_onion,
                    contact.relay_queue_id,
                    contact.expected_pubkey,
                    now,
                ],
            )
            .context("upsert_contact")?;
        let id = conn.last_insert_rowid();
        Ok(id)
    }

    pub fn list_contacts(&self) -> Result<Vec<Contact>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT label, onion_address, mls_group_id, mls_group_state,
                    relay_onion, relay_queue_id, expected_pubkey
             FROM contacts ORDER BY label",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Contact {
                    label: r.get(0)?,
                    onion_address: r.get(1)?,
                    mls_group_id: r.get(2)?,
                    mls_group_state: r.get(3)?,
                    relay_onion: r.get(4)?,
                    relay_queue_id: r.get(5)?,
                    expected_pubkey: r.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Borra un contacto por su onion address y todos sus mensajes asociados.
    /// Devuelve la cantidad de filas afectadas en `contacts` (0 si no existía,
    /// 1 si lo borró). Los mensajes se borran independientemente — un contacto
    /// puede no existir más y aún así limpiar histórico residual.
    ///
    /// Nota: si el contacto tenía un grupo MLS activo, el group state queda
    /// huérfano en el MLS storage; eso es una fuga menor que se podría limpiar
    /// con `MlsGroup::delete` desde balchat-core, pero requiere acceso a la
    /// `Identity`. Lo dejamos para una iteración posterior — el `mls_group_id`
    /// se va junto con el contact row, así que `contact_for_group_id` no
    /// vuelve a resolver mensajes a un contacto borrado.
    pub fn delete_contact_and_messages(&self, onion: &str) -> Result<usize> {
        let conn = self.conn();
        conn.execute("DELETE FROM messages WHERE contact_onion = ?1", params![onion])
            .with_context(|| format!("delete messages for {onion}"))?;
        let n = conn
            .execute("DELETE FROM contacts WHERE onion_address = ?1", params![onion])
            .with_context(|| format!("delete contact {onion}"))?;
        Ok(n)
    }

    pub fn get_contact_by_onion(&self, onion: &str) -> Result<Option<Contact>> {
        self.conn()
            .query_row(
                "SELECT label, onion_address, mls_group_id, mls_group_state,
                        relay_onion, relay_queue_id, expected_pubkey
                 FROM contacts WHERE onion_address = ?1",
                params![onion],
                |r| {
                    Ok(Contact {
                        label: r.get(0)?,
                        onion_address: r.get(1)?,
                        mls_group_id: r.get(2)?,
                        mls_group_state: r.get(3)?,
                        relay_onion: r.get(4)?,
                        relay_queue_id: r.get(5)?,
                        expected_pubkey: r.get(6)?,
                    })
                },
            )
            .optional_ext()
    }

    /// Devuelve el último seq que ya hemos descargado de un (relay, queue_id) propio.
    /// Si nunca hemos polled, retorna 0.
    pub fn get_last_seq(&self, relay_onion: &str, queue_id: &[u8]) -> Result<u64> {
        let row: Option<i64> = self
            .conn()
            .query_row(
                "SELECT last_seq FROM relay_state WHERE relay_onion = ?1 AND queue_id = ?2",
                params![relay_onion, queue_id],
                |r| r.get(0),
            )
            .optional_ext()?;
        Ok(row.unwrap_or(0) as u64)
    }

    // ---------- Messages (historial de chat) ----------

    /// Guarda un mensaje en el histórico. `direction` debe ser "sent" o "received";
    /// `kind` debe ser "text" o "file" (validados antes por el caller). `body` es el
    /// texto en text, o el filename en file.
    pub fn insert_message(
        &self,
        contact_onion: &str,
        direction: &str,
        kind: &str,
        body: &str,
    ) -> Result<i64> {
        let now = unix_now();
        let conn = self.conn();
        conn.execute(
            "INSERT INTO messages (contact_onion, direction, kind, body, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![contact_onion, direction, kind, body, now],
        )
        .with_context(|| format!("insert_message({contact_onion}, {direction}, {kind})"))?;
        Ok(conn.last_insert_rowid())
    }

    /// Devuelve los últimos `limit` mensajes del contacto, en orden cronológico
    /// ascendente (más viejo → más nuevo). Si `limit == 0`, devuelve todos.
    pub fn list_messages(&self, contact_onion: &str, limit: u32) -> Result<Vec<StoredMessage>> {
        let conn = self.conn();
        // Truco: si pedimos los últimos N, ordenamos DESC por created_at, sacamos N,
        // y luego invertimos en Rust. Más simple que CTE + ORDER BY.
        let (sql, params): (&str, Vec<Box<dyn rusqlite::ToSql>>) = if limit == 0 {
            (
                "SELECT id, contact_onion, direction, kind, body, created_at
                 FROM messages WHERE contact_onion = ?1
                 ORDER BY created_at ASC, id ASC",
                vec![Box::new(contact_onion.to_string())],
            )
        } else {
            (
                "SELECT id, contact_onion, direction, kind, body, created_at
                 FROM messages WHERE contact_onion = ?1
                 ORDER BY created_at DESC, id DESC LIMIT ?2",
                vec![
                    Box::new(contact_onion.to_string()),
                    Box::new(limit as i64),
                ],
            )
        };
        let mut stmt = conn.prepare(sql)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|b| b.as_ref()).collect();
        let mut rows: Vec<StoredMessage> = stmt
            .query_map(params_refs.as_slice(), |r| {
                Ok(StoredMessage {
                    id: r.get(0)?,
                    contact_onion: r.get(1)?,
                    direction: r.get(2)?,
                    kind: r.get(3)?,
                    body: r.get(4)?,
                    created_at: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if limit != 0 {
            rows.reverse(); // DESC → ASC
        }
        Ok(rows)
    }

    // ---------- Groups (MLS de 2+ miembros) ----------

    /// Crea un grupo en la tabla `groups`. Falla si `label` o `mls_group_id` ya existen.
    pub fn create_group(&self, label: &str, mls_group_id: &[u8]) -> Result<i64> {
        let now = unix_now();
        let conn = self.conn();
        conn.execute(
            "INSERT INTO groups (label, mls_group_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?3)",
            params![label, mls_group_id, now],
        )
        .with_context(|| format!("create_group({label})"))?;
        Ok(conn.last_insert_rowid())
    }

    pub fn list_groups(&self) -> Result<Vec<Group>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT label, mls_group_id, created_at FROM groups ORDER BY label",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Group {
                    label: r.get(0)?,
                    mls_group_id: r.get(1)?,
                    created_at: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn get_group_by_label(&self, label: &str) -> Result<Option<Group>> {
        self.conn()
            .query_row(
                "SELECT label, mls_group_id, created_at FROM groups WHERE label = ?1",
                params![label],
                |r| {
                    Ok(Group {
                        label: r.get(0)?,
                        mls_group_id: r.get(1)?,
                        created_at: r.get(2)?,
                    })
                },
            )
            .optional_ext()
    }

    pub fn get_group_by_mls_id(&self, mls_group_id: &[u8]) -> Result<Option<Group>> {
        self.conn()
            .query_row(
                "SELECT label, mls_group_id, created_at FROM groups WHERE mls_group_id = ?1",
                params![mls_group_id],
                |r| {
                    Ok(Group {
                        label: r.get(0)?,
                        mls_group_id: r.get(1)?,
                        created_at: r.get(2)?,
                    })
                },
            )
            .optional_ext()
    }

    pub fn add_group_member(&self, group_label: &str, peer_onion: &str) -> Result<()> {
        let now = unix_now();
        self.conn()
            .execute(
                "INSERT INTO group_members (group_label, peer_onion, added_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(group_label, peer_onion) DO NOTHING",
                params![group_label, peer_onion, now],
            )
            .context("add_group_member")?;
        Ok(())
    }

    pub fn list_group_members(&self, group_label: &str) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT peer_onion FROM group_members WHERE group_label = ?1 ORDER BY peer_onion")?;
        let rows = stmt
            .query_map(params![group_label], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn set_last_seq(&self, relay_onion: &str, queue_id: &[u8], last_seq: u64) -> Result<()> {
        let now = unix_now();
        self.conn()
            .execute(
                "INSERT INTO relay_state (relay_onion, queue_id, last_seq, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(relay_onion, queue_id) DO UPDATE SET
                     last_seq = excluded.last_seq, updated_at = excluded.updated_at",
                params![relay_onion, queue_id, last_seq as i64, now],
            )
            .context("set_last_seq")?;
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct Contact {
    pub label: String,
    pub onion_address: String,
    pub mls_group_id: Option<Vec<u8>>,
    pub mls_group_state: Option<Vec<u8>>,
    pub relay_onion: Option<String>,
    pub relay_queue_id: Option<Vec<u8>>,
    /// Signing key MLS esperada para este peer (verificación cross-sign opcional).
    /// Si está set, los handshakes nuevos rechazan KeyPackages de otra signing key.
    pub expected_pubkey: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct Group {
    pub label: String,
    pub mls_group_id: Vec<u8>,
    pub created_at: i64,
}

/// Una entrada del histórico de chat. Persistido en la tabla `messages`.
///
/// Para `kind == "file"`, `body` guarda el filename (los bytes del archivo
/// se guardan aparte en disco en `<base_dir>/inbox/`).
#[derive(Debug, Clone)]
pub struct StoredMessage {
    pub id: i64,
    pub contact_onion: String,
    pub direction: String,
    pub kind: String,
    pub body: String,
    pub created_at: i64,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS kv (
    key        TEXT PRIMARY KEY,
    value      BLOB NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS contacts (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    label           TEXT NOT NULL,
    onion_address   TEXT NOT NULL UNIQUE,
    mls_group_id    BLOB,
    mls_group_state BLOB,
    relay_onion     TEXT,
    relay_queue_id  BLOB,
    expected_pubkey BLOB,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS contacts_label_idx ON contacts(label);

CREATE TABLE IF NOT EXISTS relay_state (
    relay_onion  TEXT NOT NULL,
    queue_id     BLOB NOT NULL,
    last_seq     INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    PRIMARY KEY (relay_onion, queue_id)
);

CREATE TABLE IF NOT EXISTS groups (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    label         TEXT NOT NULL UNIQUE,
    mls_group_id  BLOB NOT NULL UNIQUE,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS group_members (
    group_label TEXT NOT NULL,
    peer_onion  TEXT NOT NULL,
    added_at    INTEGER NOT NULL,
    PRIMARY KEY (group_label, peer_onion)
);

-- Histórico de mensajes (texto + descriptores de archivo) por contacto.
-- `kind` ∈ {'text','file'}; `direction` ∈ {'sent','received'}.
-- `body` guarda el texto en text, o el filename en file (los bytes del archivo
-- viven aparte en disco; aquí solo persistimos el descriptor para el chat log).
CREATE TABLE IF NOT EXISTS messages (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    contact_onion TEXT NOT NULL,
    direction     TEXT NOT NULL,
    kind          TEXT NOT NULL,
    body          TEXT NOT NULL,
    created_at    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS messages_contact_created_idx
    ON messages(contact_onion, created_at);
"#;

/// ALTER TABLE migrations idempotentes para vaults antiguos (Fase 1b → 2c).
/// Cada ALTER que ya está aplicado fallará silenciosamente con "duplicate column".
fn apply_migrations(conn: &Connection) -> Result<()> {
    let migrations = [
        "ALTER TABLE contacts ADD COLUMN relay_onion TEXT",
        "ALTER TABLE contacts ADD COLUMN relay_queue_id BLOB",
        "ALTER TABLE contacts ADD COLUMN expected_pubkey BLOB",
    ];
    for sql in migrations {
        match conn.execute(sql, []) {
            Ok(_) => tracing::info!("applied: {sql}"),
            Err(rusqlite::Error::SqliteFailure(_, Some(msg)))
                if msg.contains("duplicate column") => {}
            Err(e) => return Err(anyhow!("migración falló ({sql}): {e}")),
        }
    }
    // CREATE TABLE IF NOT EXISTS — para bases antiguas que aún no las tienen.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS relay_state (
            relay_onion  TEXT NOT NULL,
            queue_id     BLOB NOT NULL,
            last_seq     INTEGER NOT NULL,
            updated_at   INTEGER NOT NULL,
            PRIMARY KEY (relay_onion, queue_id)
        );

        CREATE TABLE IF NOT EXISTS groups (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            label         TEXT NOT NULL UNIQUE,
            mls_group_id  BLOB NOT NULL UNIQUE,
            created_at    INTEGER NOT NULL,
            updated_at    INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS group_members (
            group_label TEXT NOT NULL,
            peer_onion  TEXT NOT NULL,
            added_at    INTEGER NOT NULL,
            PRIMARY KEY (group_label, peer_onion)
        );

        CREATE TABLE IF NOT EXISTS messages (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            contact_onion TEXT NOT NULL,
            direction     TEXT NOT NULL,
            kind          TEXT NOT NULL,
            body          TEXT NOT NULL,
            created_at    INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS messages_contact_created_idx
            ON messages(contact_onion, created_at);",
    )?;
    Ok(())
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Escapa una passphrase para usarla como literal SQL string en `PRAGMA key = '...'`.
/// SQLCipher acepta también `PRAGMA key = "x'<hex>'"`; aquí usamos el formato literal.
fn quote_sql_string(s: &str) -> String {
    let escaped = s.replace('\'', "''");
    format!("'{escaped}'")
}

fn salt_path_for(vault_path: &Path) -> PathBuf {
    // Adyacente al vault: `vault.db.salt`. No es secreto, no lo cifrarmos.
    let mut s = vault_path.as_os_str().to_owned();
    s.push(".salt");
    PathBuf::from(s)
}

fn derive_argon2_key(passphrase: &[u8], salt: &[u8]) -> Result<[u8; ARGON2_KEY_LEN]> {
    let params = Params::new(
        ARGON2_M_COST,
        ARGON2_T_COST,
        ARGON2_P_COST,
        Some(ARGON2_KEY_LEN),
    )
    .map_err(|e| anyhow!("Argon2 params: {e:?}"))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; ARGON2_KEY_LEN];
    argon2
        .hash_password_into(passphrase, salt, &mut key)
        .map_err(|e| anyhow!("Argon2 hash: {e:?}"))?;
    Ok(key)
}

trait OptionalExt<T> {
    fn optional_ext(self) -> Result<Option<T>>;
}

impl<T> OptionalExt<T> for rusqlite::Result<T> {
    fn optional_ext(self) -> Result<Option<T>> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow!(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_db() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("balchat-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(format!("{}.db", uuid_like()))
    }

    fn uuid_like() -> String {
        // Mezclamos nanos con bytes aleatorios para evitar colisiones cuando varios
        // tests parallel reservan filenames en el mismo nanosegundo.
        use rand::RngCore;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut rnd = [0u8; 8];
        rand::thread_rng().fill_bytes(&mut rnd);
        let rnd_hex: String = rnd.iter().map(|b| format!("{b:02x}")).collect();
        format!("{nanos:x}-{rnd_hex}")
    }

    #[test]
    fn create_open_kv_roundtrip() -> Result<()> {
        let path = tmp_db();
        let salt = salt_path_for(&path);
        {
            let v = Vault::open(&path, "supersecret").unwrap();
            v.kv_set("identity", b"hello world").unwrap();
            v.upsert_contact(&Contact {
                label: "bob".into(),
                onion_address: "abc.onion:1234".into(),
                ..Default::default()
            })
            .unwrap();
        }
        // Confirmamos que se generó el salt file (modo Argon2id en vaults nuevos).
        assert!(salt.exists(), "salt file debió crearse en {}", salt.display());
        assert_eq!(std::fs::read(&salt)?.len(), ARGON2_SALT_LEN);

        let v = Vault::open(&path, "supersecret").unwrap();
        let val = v.kv_get("identity")?.unwrap();
        assert_eq!(val, b"hello world");
        let contacts = v.list_contacts()?;
        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts[0].label, "bob");

        std::fs::remove_file(&path).ok();
        std::fs::remove_file(&salt).ok();
        Ok(())
    }

    #[test]
    fn wrong_passphrase_rejected() {
        let path = tmp_db();
        let salt = salt_path_for(&path);
        {
            let v = Vault::open(&path, "correct").unwrap();
            v.kv_set("k", b"v").unwrap();
        }
        let err = match Vault::open(&path, "incorrect") {
            Err(e) => e,
            Ok(_) => panic!("debió fallar con passphrase incorrecta"),
        };
        let msg = format!("{err:#}");
        assert!(
            msg.contains("not a database") || msg.contains("file is not a database") || msg.contains("encrypted"),
            "esperaba error de descifrado, llegó: {msg}"
        );
        std::fs::remove_file(&path).ok();
        std::fs::remove_file(&salt).ok();
    }

    #[test]
    fn passphrase_with_quotes_works() -> Result<()> {
        let path = tmp_db();
        let salt = salt_path_for(&path);
        let weird = "mi'pass\"con\\caracteres'raros";
        {
            let v = Vault::open(&path, weird)?;
            v.kv_set("k", b"v")?;
        }
        let v = Vault::open(&path, weird)?;
        assert_eq!(v.kv_get("k")?.unwrap(), b"v");
        std::fs::remove_file(&path).ok();
        std::fs::remove_file(&salt).ok();
        Ok(())
    }

    /// Vaults legacy sin salt file siguen abriendo con passphrase directa.
    /// Construimos un vault legacy a mano (PRAGMA key clásico) y verificamos.
    #[test]
    fn legacy_vault_without_salt_still_opens() -> Result<()> {
        use rusqlite::Connection;
        let path = tmp_db();
        let salt_p = salt_path_for(&path);
        // Aseguramos que NO hay salt file.
        let _ = std::fs::remove_file(&salt_p);

        // Crear vault legacy: connexion + PRAGMA key con passphrase directa.
        {
            let conn = Connection::open_with_flags(
                &path,
                OpenFlags::SQLITE_OPEN_READ_WRITE
                    | OpenFlags::SQLITE_OPEN_CREATE
                    | OpenFlags::SQLITE_OPEN_URI
                    | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
            )?;
            conn.execute_batch(&format!(
                "PRAGMA key = {};\nPRAGMA cipher_compatibility = 4;",
                quote_sql_string("legacy-pass")
            ))?;
            conn.execute_batch(SCHEMA)?;
            conn.execute(
                "INSERT INTO kv (key, value, updated_at) VALUES (?1, ?2, ?3)",
                rusqlite::params!["k", b"legacy-data".as_slice(), unix_now()],
            )?;
        }
        assert!(!salt_p.exists(), "vault legacy NO debe tener salt file");

        // Vault::open debe detectar ausencia de salt → modo legacy → abrir OK.
        let v = Vault::open(&path, "legacy-pass")?;
        let val = v.kv_get("k")?.unwrap();
        assert_eq!(val, b"legacy-data");

        std::fs::remove_file(&path).ok();
        Ok(())
    }

    /// Roundtrip de la tabla `messages`: insertamos sent+received para dos contactos
    /// y verificamos que `list_messages` devuelve sólo los del contacto pedido,
    /// en orden cronológico ascendente, y respeta el `limit`.
    #[test]
    fn messages_insert_list_roundtrip() -> Result<()> {
        let path = tmp_db();
        let salt = salt_path_for(&path);
        let v = Vault::open(&path, "pw")?;

        // Mezclamos contactos a propósito para testear el filtro por onion.
        v.insert_message("a.onion:1234", "sent", "text", "hola alice")?;
        v.insert_message("b.onion:1234", "sent", "text", "hola bob")?;
        v.insert_message("a.onion:1234", "received", "text", "que tal")?;
        v.insert_message("a.onion:1234", "sent", "file", "doc.pdf")?;

        let alice = v.list_messages("a.onion:1234", 0)?;
        assert_eq!(alice.len(), 3);
        assert_eq!(alice[0].body, "hola alice");
        assert_eq!(alice[1].body, "que tal");
        assert_eq!(alice[1].direction, "received");
        assert_eq!(alice[2].kind, "file");
        assert_eq!(alice[2].body, "doc.pdf");

        let bob = v.list_messages("b.onion:1234", 0)?;
        assert_eq!(bob.len(), 1);
        assert_eq!(bob[0].body, "hola bob");

        // Verificamos que el limit pequeño se queda con los más recientes en
        // orden ascendente (sólo los últimos N).
        let last_two = v.list_messages("a.onion:1234", 2)?;
        assert_eq!(last_two.len(), 2);
        assert_eq!(last_two[0].body, "que tal");
        assert_eq!(last_two[1].body, "doc.pdf");

        std::fs::remove_file(&path).ok();
        std::fs::remove_file(&salt).ok();
        Ok(())
    }

    /// Verifica que `delete_contact_and_messages` borra el contact row y los
    /// mensajes asociados en cascada, sin tocar a otros contactos.
    #[test]
    fn delete_contact_cascades_messages() -> Result<()> {
        let path = tmp_db();
        let salt = salt_path_for(&path);
        let v = Vault::open(&path, "pw")?;
        v.upsert_contact(&Contact {
            label: "alice".into(),
            onion_address: "a.onion:1234".into(),
            ..Default::default()
        })?;
        v.upsert_contact(&Contact {
            label: "bob".into(),
            onion_address: "b.onion:1234".into(),
            ..Default::default()
        })?;
        v.insert_message("a.onion:1234", "sent", "text", "1")?;
        v.insert_message("a.onion:1234", "received", "text", "2")?;
        v.insert_message("b.onion:1234", "sent", "text", "para bob")?;

        let n = v.delete_contact_and_messages("a.onion:1234")?;
        assert_eq!(n, 1, "debió borrar 1 contact row");
        assert_eq!(v.list_contacts()?.len(), 1, "queda solo bob");
        assert!(v.list_messages("a.onion:1234", 0)?.is_empty(), "alice sin msgs");
        let bob_msgs = v.list_messages("b.onion:1234", 0)?;
        assert_eq!(bob_msgs.len(), 1, "bob no debe ser afectado");
        assert_eq!(bob_msgs[0].body, "para bob");

        // Borrar un contacto inexistente no es error pero retorna 0.
        let n2 = v.delete_contact_and_messages("does-not-exist.onion:1234")?;
        assert_eq!(n2, 0);

        std::fs::remove_file(&path).ok();
        std::fs::remove_file(&salt).ok();
        Ok(())
    }
}
