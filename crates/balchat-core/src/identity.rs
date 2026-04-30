//! Identidad MLS local. En Fase 1a vive en memoria; en 1b irá a SQLCipher.

use anyhow::{Context, Result};
use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::OpenMlsProvider;
use serde::{Deserialize, Serialize};

pub const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

/// Una identidad MLS: provider de cripto + clave de firma + credencial.
///
/// Esta es solo la parte MLS. La identidad de transporte (clave del onion v3) la gestiona
/// `tor-hsservice` separadamente; en una versión posterior las atamos vía cross-signing.
pub struct Identity {
    pub label: String,
    pub credential: CredentialWithKey,
    pub signer: SignatureKeyPair,
    pub provider: OpenMlsRustCrypto,
}

impl Identity {
    pub fn new(label: &str) -> Result<Self> {
        let provider = OpenMlsRustCrypto::default();

        let basic = BasicCredential::new(label.as_bytes().to_vec());
        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())
            .context("crear SignatureKeyPair")?;
        signer
            .store(provider.storage())
            .context("guardar signer en storage")?;

        let credential = CredentialWithKey {
            credential: basic.into(),
            signature_key: signer.public().into(),
        };

        Ok(Self {
            label: label.to_string(),
            credential,
            signer,
            provider,
        })
    }

    /// Genera un KeyPackage fresco para ser invitado a un grupo.
    pub fn fresh_key_package(&self) -> Result<KeyPackageBundle> {
        KeyPackage::builder()
            .build(
                CIPHERSUITE,
                &self.provider,
                &self.signer,
                self.credential.clone(),
            )
            .context("KeyPackage::builder().build()")
    }

    /// Serializa label + dump del storage interno del provider MLS.
    ///
    /// Las claves privadas NO están expuestas por el API público de openmls — pero el
    /// `MemoryStorage` interno (`OpenMlsRustCrypto::storage().values`) es un `HashMap`
    /// público que contiene TODO el estado MLS de esta identidad (signature key, key
    /// packages, group state). Persistimos ese mapa entero como blob CBOR cifrado
    /// dentro del vault SQLCipher.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let storage = self.provider.storage();
        let map = storage
            .values
            .read()
            .map_err(|e| anyhow::anyhow!("storage RwLock poisoned: {e}"))?;
        let dump: Vec<(Vec<u8>, Vec<u8>)> =
            map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();

        let persisted = PersistedIdentity {
            version: PERSISTED_IDENTITY_VERSION,
            label: self.label.clone(),
            signature_public_key: self.signer.public().to_vec(),
            storage_dump: dump,
        };
        let mut out = Vec::new();
        ciborium::ser::into_writer(&persisted, &mut out)
            .context("serializar Identity (CBOR)")?;
        Ok(out)
    }

    /// Reconstruye una Identity desde el blob producido por [`to_bytes`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let p: PersistedIdentity =
            ciborium::de::from_reader(bytes).context("deserializar Identity (CBOR)")?;
        if p.version != PERSISTED_IDENTITY_VERSION {
            anyhow::bail!(
                "versión de Identity persistida desconocida: {} (esperaba {PERSISTED_IDENTITY_VERSION})",
                p.version
            );
        }

        let provider = OpenMlsRustCrypto::default();
        {
            let mut map = provider
                .storage()
                .values
                .write()
                .map_err(|e| anyhow::anyhow!("storage RwLock poisoned: {e}"))?;
            for (k, v) in p.storage_dump {
                map.insert(k, v);
            }
        }

        let signer = SignatureKeyPair::read(
            provider.storage(),
            &p.signature_public_key,
            CIPHERSUITE.signature_algorithm(),
        )
        .ok_or_else(|| {
            anyhow::anyhow!("signature key no encontrada en storage tras restaurar dump")
        })?;

        let basic = BasicCredential::new(p.label.as_bytes().to_vec());
        let credential = CredentialWithKey {
            credential: basic.into(),
            signature_key: signer.public().into(),
        };

        Ok(Self {
            label: p.label,
            credential,
            signer,
            provider,
        })
    }
}

const PERSISTED_IDENTITY_VERSION: u16 = 1;
const VAULT_KEY_IDENTITY: &str = "identity.v1";
const VAULT_KEY_QUEUE_ID: &str = "queue_id.v1";
const VAULT_KEY_MY_RELAY: &str = "my_relay_onion.v1";
const QUEUE_ID_LEN: usize = 32;

#[derive(Serialize, Deserialize)]
struct PersistedIdentity {
    version: u16,
    label: String,
    #[serde(with = "serde_bytes")]
    signature_public_key: Vec<u8>,
    /// Dump completo del MemoryStorage interno del provider MLS.
    storage_dump: Vec<(Vec<u8>, Vec<u8>)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_preserves_signing_key() -> Result<()> {
        let original = Identity::new("alice")?;
        let public_before = original.signer.public().to_vec();

        let blob = original.to_bytes()?;
        let restored = Identity::from_bytes(&blob)?;

        assert_eq!(restored.label, "alice");
        assert_eq!(restored.signer.public(), &public_before[..]);
        Ok(())
    }

    #[test]
    fn restored_identity_can_create_keypackage() -> Result<()> {
        let original = Identity::new("bob")?;
        let _ = original.fresh_key_package()?;
        let blob = original.to_bytes()?;
        let restored = Identity::from_bytes(&blob)?;
        let _ = restored.fresh_key_package()?;
        Ok(())
    }
}

/// Carga la Identity del vault o, si no existe, crea una nueva con `label` y la persiste.
pub fn load_or_create(vault: &balchat_storage::Vault, label: &str) -> Result<Identity> {
    if let Some(blob) = vault.kv_get(VAULT_KEY_IDENTITY)? {
        tracing::info!("identidad existente cargada del vault");
        Identity::from_bytes(&blob)
    } else {
        tracing::info!("creando identidad nueva con label '{label}'");
        let identity = Identity::new(label)?;
        let blob = identity.to_bytes()?;
        vault.kv_set(VAULT_KEY_IDENTITY, &blob)?;
        Ok(identity)
    }
}

/// Re-serializa la identidad al vault. Llamar tras cambios al MLS state
/// (handshake, send/recv) para no perder ratchet progress.
pub fn save(vault: &balchat_storage::Vault, identity: &Identity) -> Result<()> {
    let blob = identity.to_bytes()?;
    vault.kv_set(VAULT_KEY_IDENTITY, &blob)?;
    Ok(())
}

/// Carga el queue_id de este vault. Si no existe, genera uno aleatorio (32 bytes)
/// y lo persiste. El queue_id es el identificador en relays — los peers que quieran
/// dejarme mensajes offline necesitan conocer este id.
pub fn load_or_create_queue_id(vault: &balchat_storage::Vault) -> Result<Vec<u8>> {
    if let Some(b) = vault.kv_get(VAULT_KEY_QUEUE_ID)? {
        if b.len() == QUEUE_ID_LEN {
            return Ok(b);
        }
        anyhow::bail!("queue_id en vault tiene longitud incorrecta: {}", b.len());
    }
    use rand::RngCore;
    let mut q = vec![0u8; QUEUE_ID_LEN];
    rand::thread_rng().fill_bytes(&mut q);
    vault.kv_set(VAULT_KEY_QUEUE_ID, &q)?;
    Ok(q)
}

pub fn get_my_relay(vault: &balchat_storage::Vault) -> Result<Option<String>> {
    Ok(vault
        .kv_get(VAULT_KEY_MY_RELAY)?
        .map(|b| String::from_utf8_lossy(&b).into_owned()))
}

pub fn set_my_relay(vault: &balchat_storage::Vault, relay_onion: &str) -> Result<()> {
    vault.kv_set(VAULT_KEY_MY_RELAY, relay_onion.as_bytes())
}

/// Crea un nuevo MlsGroup con sólo el caller dentro. Para invitar peers después,
/// usa [`crate::conversation::invite_peer_to_existing_group`].
pub fn create_group(identity: &Identity) -> Result<MlsGroup> {
    let cfg = MlsGroupCreateConfig::builder()
        .use_ratchet_tree_extension(true)
        .build();
    MlsGroup::new(
        &identity.provider,
        &identity.signer,
        &cfg,
        identity.credential.clone(),
    )
    .context("MlsGroup::new (create_group)")
}

/// Carga un MlsGroup desde el storage del provider. Útil para `send-group`
/// y `invite` que necesitan operar sobre un grupo persistido.
pub fn load_group(identity: &Identity, group_id: &[u8]) -> Result<MlsGroup> {
    MlsGroup::load(
        identity.provider.storage(),
        &openmls::group::GroupId::from_slice(group_id),
    )
    .map_err(|e| anyhow::anyhow!("MlsGroup::load: {e:?}"))?
    .ok_or_else(|| anyhow::anyhow!("group_id no encontrado en storage MLS"))
}

/// Procesa un blob que se asume ser un MLS Welcome — joinea el grupo y persiste
/// el state. Devuelve el `group_id` del grupo recién joineado.
pub fn process_welcome_blob(identity: &Identity, blob: &[u8]) -> Result<Vec<u8>> {
    use openmls::prelude::{MlsGroupJoinConfig, MlsMessageBodyIn, MlsMessageIn, StagedWelcome};

    let in_msg = MlsMessageIn::tls_deserialize_exact_bytes(blob)
        .context("deserializar MlsMessageIn (Welcome esperado)")?;
    let welcome = match in_msg.extract() {
        MlsMessageBodyIn::Welcome(w) => w,
        other => anyhow::bail!("se esperaba Welcome, llegó {other:?}"),
    };

    let staged = StagedWelcome::new_from_welcome(
        &identity.provider,
        &MlsGroupJoinConfig::default(),
        welcome,
        None,
    )
    .context("StagedWelcome::new_from_welcome (offline)")?;
    let group = staged
        .into_group(&identity.provider)
        .context("StagedWelcome::into_group (offline)")?;
    Ok(group.group_id().as_slice().to_vec())
}

/// Helper: indica si un blob MLS es un Welcome (sin consumirlo del provider).
pub fn blob_is_welcome(blob: &[u8]) -> bool {
    use openmls::prelude::{MlsMessageBodyIn, MlsMessageIn};

    match MlsMessageIn::tls_deserialize_exact_bytes(blob) {
        Ok(in_msg) => matches!(in_msg.extract(), MlsMessageBodyIn::Welcome(_)),
        Err(_) => false,
    }
}
