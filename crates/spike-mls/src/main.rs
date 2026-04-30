//! balchat — Fase 0 spike: openmls 1:1 handshake + cifrado simétrico de mensajes.
//!
//! Demuestra:
//!   1. Dos identidades MLS independientes (Alice, Bob) con BasicCredential+Ed25519.
//!   2. Bob publica un KeyPackage; Alice crea un grupo y lo añade vía add_members.
//!   3. Bob procesa el Welcome y entra al grupo.
//!   4. Intercambio bidireccional de un ApplicationMessage cifrado.
//!
//! Sin red — todo en proceso, intercambio de bytes vía variables locales.

use anyhow::{anyhow, Context, Result};
use openmls::prelude::*;
use openmls::prelude::tls_codec::Serialize as _;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::OpenMlsProvider;

const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

/// Una identidad MLS local: provider de cripto, claves de firma, y credencial.
/// En balchat real, `provider` estará respaldado por SQLCipher en disco; aquí va en memoria.
struct Identity {
    name: String,
    credential: CredentialWithKey,
    signer: SignatureKeyPair,
    provider: OpenMlsRustCrypto,
}

impl Identity {
    fn new(name: &str) -> Result<Self> {
        let provider = OpenMlsRustCrypto::default();

        let credential = BasicCredential::new(name.as_bytes().to_vec());
        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())
            .context("crear SignatureKeyPair")?;

        // Persistir la clave de firma en el storage del provider para que MLS la pueda usar.
        signer
            .store(provider.storage())
            .context("guardar signer en storage")?;

        let credential_with_key = CredentialWithKey {
            credential: credential.into(),
            signature_key: signer.public().into(),
        };

        Ok(Self {
            name: name.to_string(),
            credential: credential_with_key,
            signer,
            provider,
        })
    }

    /// Genera un KeyPackage que esta identidad puede publicar para ser invitada a grupos.
    fn fresh_key_package(&self) -> Result<KeyPackageBundle> {
        KeyPackage::builder()
            .build(
                CIPHERSUITE,
                &self.provider,
                &self.signer,
                self.credential.clone(),
            )
            .context("construir KeyPackage")
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .init();

    println!("=== balchat — spike MLS ===\n");

    // -------- Setup: dos partes con MLS state aislado --------
    let alice = Identity::new("alice")?;
    let bob = Identity::new("bob")?;
    println!("[1] Identidades creadas (BasicCredential + Ed25519)");

    // -------- Bob publica un KeyPackage --------
    // En balchat real, este blob viajaría por el relay no-confiable.
    let bob_kp_bundle = bob.fresh_key_package()?;
    let bob_kp_for_alice: KeyPackage = bob_kp_bundle.key_package().clone();

    let bob_kp_wire = bob_kp_for_alice
        .tls_serialize_detached()
        .context("serializar KeyPackage de Bob")?;
    println!("[2] KeyPackage de Bob serializado: {} bytes", bob_kp_wire.len());

    // -------- Alice crea un grupo conteniéndose a sí misma --------
    // use_ratchet_tree_extension(true) hace que los Welcome incluyan el ratchet tree;
    // simplifica el spike (sin canal lateral). En producción puede convenir mandarlo
    // por separado para reducir tamaño del Welcome cuando el grupo es grande.
    let create_cfg = MlsGroupCreateConfig::builder()
        .use_ratchet_tree_extension(true)
        .build();
    let mut alice_group = MlsGroup::new(
        &alice.provider,
        &alice.signer,
        &create_cfg,
        alice.credential.clone(),
    )
    .context("Alice MlsGroup::new")?;
    println!(
        "[3] Alice crea grupo (epoch {})",
        alice_group.epoch().as_u64()
    );

    // -------- Alice añade a Bob (commit + welcome) --------
    // Recibe el KeyPackage del wire.
    let bob_kp_received = KeyPackageIn::tls_deserialize_exact_bytes(&bob_kp_wire)
        .context("deserializar KeyPackage de Bob")?
        .validate(alice.provider.crypto(), ProtocolVersion::Mls10)
        .context("validar KeyPackage de Bob")?;

    let (_commit, welcome_out, _group_info) = alice_group
        .add_members(&alice.provider, &alice.signer, &[bob_kp_received])
        .context("add_members")?;

    alice_group
        .merge_pending_commit(&alice.provider)
        .context("merge_pending_commit")?;
    println!(
        "[4] Alice añade a Bob (epoch ahora {})",
        alice_group.epoch().as_u64()
    );

    // El Welcome viaja a Bob por el canal fuera de banda.
    let welcome_wire = welcome_out
        .tls_serialize_detached()
        .context("serializar Welcome")?;
    println!("    Welcome serializado: {} bytes", welcome_wire.len());

    // -------- Bob procesa Welcome y se une al grupo --------
    let welcome_in = MlsMessageIn::tls_deserialize_exact_bytes(&welcome_wire)
        .context("deserializar MlsMessageIn (welcome)")?;

    let welcome = match welcome_in.extract() {
        MlsMessageBodyIn::Welcome(w) => w,
        other => return Err(anyhow!("se esperaba Welcome, llegó {:?}", other)),
    };

    let staged = StagedWelcome::new_from_welcome(
        &bob.provider,
        &MlsGroupJoinConfig::default(),
        welcome,
        None, // ratchet_tree opcional (lo trae el Welcome)
    )
    .context("StagedWelcome::new_from_welcome")?;

    let mut bob_group = staged
        .into_group(&bob.provider)
        .context("StagedWelcome::into_group")?;
    println!(
        "[5] Bob entra al grupo (epoch {})",
        bob_group.epoch().as_u64()
    );

    assert_eq!(alice_group.group_id(), bob_group.group_id(), "group_id mismatch");
    assert_eq!(alice_group.epoch(), bob_group.epoch(), "epoch mismatch");

    // -------- Alice -> Bob: ApplicationMessage cifrado --------
    let plain_a = b"hola bob, esto es balchat sobre MLS";
    let ct_a = alice_group
        .create_message(&alice.provider, &alice.signer, plain_a)
        .context("Alice create_message")?;
    let wire_a = ct_a.tls_serialize_detached()?;
    println!(
        "[6] Alice cifra '{}': {} bytes en wire",
        String::from_utf8_lossy(plain_a),
        wire_a.len()
    );

    let in_a = MlsMessageIn::tls_deserialize_exact_bytes(&wire_a)?;
    let proto_a: ProtocolMessage = in_a
        .try_into_protocol_message()
        .map_err(|_| anyhow!("no es ProtocolMessage"))?;
    let processed_a = bob_group
        .process_message(&bob.provider, proto_a)
        .context("Bob process_message")?;

    match processed_a.into_content() {
        ProcessedMessageContent::ApplicationMessage(app) => {
            let recovered = app.into_bytes();
            println!(
                "[7] Bob descifra: {:?}",
                String::from_utf8_lossy(&recovered)
            );
            assert_eq!(recovered, plain_a);
        }
        other => return Err(anyhow!("contenido inesperado: {:?}", other)),
    }

    // -------- Bob -> Alice: respuesta cifrada --------
    let plain_b = b"hola alice, recibido y descifrado correctamente";
    let ct_b = bob_group.create_message(&bob.provider, &bob.signer, plain_b)?;
    let wire_b = ct_b.tls_serialize_detached()?;

    let in_b = MlsMessageIn::tls_deserialize_exact_bytes(&wire_b)?;
    let proto_b: ProtocolMessage = in_b
        .try_into_protocol_message()
        .map_err(|_| anyhow!("no es ProtocolMessage"))?;
    let processed_b = alice_group.process_message(&alice.provider, proto_b)?;
    match processed_b.into_content() {
        ProcessedMessageContent::ApplicationMessage(app) => {
            let recovered = app.into_bytes();
            println!(
                "[8] Alice descifra respuesta: {:?}",
                String::from_utf8_lossy(&recovered)
            );
            assert_eq!(recovered, plain_b);
        }
        other => return Err(anyhow!("contenido inesperado: {:?}", other)),
    }

    println!("\n[OK] handshake MLS + dos ApplicationMessage cifrados ida y vuelta.");
    println!("    Estos son los primitivos sobre los que balchat construirá 1:1 y, después, grupos.");

    let _ = alice.name;
    let _ = bob.name;
    Ok(())
}
