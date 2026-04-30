//! Cliente del relay no-confiable balchat.
//!
//! Cada operación abre un stream Tor nuevo, manda 1 request, lee 1 response, cierra.

use anyhow::{anyhow, Context, Result};
use balchat_relay_proto::{
    recv_frame, send_frame, QueueMessage, RelayRequest, RelayResponse, PROTOCOL_VERSION,
    VIRTUAL_PORT,
};

use crate::transport::Endpoint;

pub struct RelayClient<'a> {
    endpoint: &'a Endpoint,
}

impl<'a> RelayClient<'a> {
    pub fn new(endpoint: &'a Endpoint) -> Self {
        Self { endpoint }
    }

    /// Deposita un blob cifrado en `<relay_onion>` para el `queue_id` dado.
    /// Devuelve el `seq` que el relay asignó.
    pub async fn put(
        &self,
        relay_onion: &str,
        queue_id: &[u8],
        blob: Vec<u8>,
    ) -> Result<u64> {
        let target = with_relay_port(relay_onion);
        let mut stream = self.endpoint.dial(&target).await
            .with_context(|| format!("dial relay {target}"))?;

        let req = RelayRequest::Put {
            protocol_version: PROTOCOL_VERSION,
            queue_id: queue_id.to_vec(),
            blob,
        };
        send_frame(&mut stream, &req).await.context("send Put")?;
        let resp: RelayResponse = recv_frame(&mut stream).await.context("recv ack")?;
        match resp {
            RelayResponse::PutAck { seq } => Ok(seq),
            RelayResponse::Error { msg } => Err(anyhow!("relay error: {msg}")),
            other => Err(anyhow!("respuesta inesperada del relay: {other:?}")),
        }
    }

    /// Descarga blobs con `seq > since_seq` para el `queue_id` (hasta `max`).
    pub async fn get(
        &self,
        relay_onion: &str,
        queue_id: &[u8],
        since_seq: u64,
        max: u32,
    ) -> Result<Vec<QueueMessage>> {
        let target = with_relay_port(relay_onion);
        let mut stream = self.endpoint.dial(&target).await
            .with_context(|| format!("dial relay {target}"))?;

        let req = RelayRequest::Get {
            protocol_version: PROTOCOL_VERSION,
            queue_id: queue_id.to_vec(),
            since_seq,
            max_messages: max,
        };
        send_frame(&mut stream, &req).await.context("send Get")?;
        let resp: RelayResponse = recv_frame(&mut stream).await.context("recv reply")?;
        match resp {
            RelayResponse::GetReply { messages } => Ok(messages),
            RelayResponse::Error { msg } => Err(anyhow!("relay error: {msg}")),
            other => Err(anyhow!("respuesta inesperada del relay: {other:?}")),
        }
    }

    /// Publica un KeyPackage en el pool del peer dueño de `queue_id`.
    /// El peer puede consumirlos (con [`consume_keypackage`]) para crear Welcome
    /// asincrónico cuando quiere invitarte a un grupo y vos estás offline.
    pub async fn put_keypackage(
        &self,
        relay_onion: &str,
        queue_id: &[u8],
        key_package: Vec<u8>,
    ) -> Result<u32> {
        let target = with_relay_port(relay_onion);
        let mut stream = self
            .endpoint
            .dial(&target)
            .await
            .with_context(|| format!("dial relay {target}"))?;
        let req = RelayRequest::PutKeyPackage {
            protocol_version: PROTOCOL_VERSION,
            queue_id: queue_id.to_vec(),
            key_package,
        };
        send_frame(&mut stream, &req).await.context("send PutKeyPackage")?;
        let resp: RelayResponse = recv_frame(&mut stream).await.context("recv ack")?;
        match resp {
            RelayResponse::PutKeyPackageAck { pool_size } => Ok(pool_size),
            RelayResponse::Error { msg } => Err(anyhow!("relay error: {msg}")),
            other => Err(anyhow!("respuesta inesperada: {other:?}")),
        }
    }

    /// Consume (toma + borra) un KeyPackage del pool del peer.
    /// Devuelve `None` si el pool está vacío (peer aún no publicó KPs).
    pub async fn consume_keypackage(
        &self,
        relay_onion: &str,
        queue_id: &[u8],
    ) -> Result<Option<Vec<u8>>> {
        let target = with_relay_port(relay_onion);
        let mut stream = self
            .endpoint
            .dial(&target)
            .await
            .with_context(|| format!("dial relay {target}"))?;
        let req = RelayRequest::ConsumeKeyPackage {
            protocol_version: PROTOCOL_VERSION,
            queue_id: queue_id.to_vec(),
        };
        send_frame(&mut stream, &req).await.context("send ConsumeKeyPackage")?;
        let resp: RelayResponse = recv_frame(&mut stream).await.context("recv reply")?;
        match resp {
            RelayResponse::ConsumeKeyPackageReply { key_package } => Ok(key_package),
            RelayResponse::Error { msg } => Err(anyhow!("relay error: {msg}")),
            other => Err(anyhow!("respuesta inesperada: {other:?}")),
        }
    }
}

fn with_relay_port(onion: &str) -> String {
    if onion.contains(':') {
        onion.to_string()
    } else {
        format!("{onion}:{VIRTUAL_PORT}")
    }
}
