//! balchat-core — primitivos compartidos entre CLI y futura UI.
//!
//! Tres conceptos:
//!   * [`Identity`] — claves MLS + provider de cripto (en memoria por ahora).
//!   * [`Endpoint`] — TorClient + facilidades para hospedar un onion service y dial.
//!   * [`Conversation`] — un canal MLS 1:1 sobre un `DataStream` Tor.

pub mod conversation;
pub mod identity;
pub mod relay_client;
pub mod transport;
pub mod wire;

pub use balchat_storage as storage;

pub use conversation::Conversation;
pub use identity::{Identity, CIPHERSUITE};
pub use transport::{Endpoint, HostHandle};

// HostHandle queda en el path balchat_core::HostHandle (re-export arriba) y también
// accesible como balchat_core::transport::HostHandle.

// Re-exports para que los consumidores no tengan que añadir arti-client a su Cargo.toml.
pub use arti_client::DataStream;
