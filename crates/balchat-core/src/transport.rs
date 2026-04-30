//! Transporte sobre Tor: bootstrap, hospedar onion service, dial.

use anyhow::{anyhow, Context, Result};
use arti_client::config::CfgPath;
use arti_client::{DataStream, TorClient, TorClientConfig};
use futures::StreamExt;
use safelog::DisplayRedacted;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;
use tor_hsservice::config::OnionServiceConfig;
use tor_hsservice::{handle_rend_requests, HsNickname, RunningOnionService};
use tor_rtcompat::PreferredRuntime;

/// Puerto virtual en el que balchat escucha dentro del onion service.
/// Es un puerto del HS (interno a Tor), no un puerto TCP local del host.
pub const VIRTUAL_PORT: u16 = 1234;

pub struct Endpoint {
    pub client: TorClient<PreferredRuntime>,
}

impl Endpoint {
    /// Hace bootstrap del cliente Tor con paths default (`~/Library/Application Support/arti/...`).
    /// Útil para spikes; balchat real debe usar [`bootstrap_in`] para que estado y `.onion`
    /// queden anclados al vault.
    pub async fn bootstrap() -> Result<Self> {
        let client = TorClient::create_bootstrapped(TorClientConfig::default())
            .await
            .context("TorClient::create_bootstrapped")?;
        Ok(Self { client })
    }

    /// Bootstrap usando `<base>/arti-state` y `<base>/arti-cache`. Mismo `base` entre
    /// runs ⇒ misma `.onion` (las claves del onion service se guardan en state).
    pub async fn bootstrap_in(base_dir: &Path) -> Result<Self> {
        let state_dir = base_dir.join("arti-state");
        let cache_dir = base_dir.join("arti-cache");
        create_secure_dir(&state_dir)?;
        create_secure_dir(&cache_dir)?;

        let mut builder = TorClientConfig::builder();
        builder.storage().state_dir(CfgPath::new_literal(state_dir));
        builder.storage().cache_dir(CfgPath::new_literal(cache_dir));
        let cfg = builder.build().context("TorClientConfig::build")?;

        let client = TorClient::create_bootstrapped(cfg)
            .await
            .context("TorClient::create_bootstrapped (con state_dir custom)")?;
        Ok(Self { client })
    }

    /// Levanta un onion service con el nickname dado y retorna un handle del que se
    /// pueden recibir streams entrantes ya `accept`ados.
    pub async fn host_onion(&self, nickname: &str) -> Result<HostHandle> {
        let nick: HsNickname = nickname
            .parse()
            .with_context(|| format!("HsNickname inválido: {nickname:?}"))?;
        let cfg = OnionServiceConfig::builder()
            .nickname(nick)
            .build()
            .context("OnionServiceConfig::build")?;

        let (svc, rend_stream) = self
            .client
            .launch_onion_service(cfg)
            .context("launch_onion_service")?
            .ok_or_else(|| anyhow!("onion service deshabilitado en config"))?;

        let onion = svc
            .onion_address()
            .ok_or_else(|| anyhow!("RunningOnionService sin .onion address"))?
            .display_unredacted()
            .to_string();

        let (tx, rx) = mpsc::channel::<DataStream>(8);
        tokio::spawn(async move {
            let mut requests = std::pin::pin!(handle_rend_requests(rend_stream));
            while let Some(req) = requests.next().await {
                let tx = tx.clone();
                tokio::spawn(async move {
                    use tor_cell::relaycell::msg::Connected;
                    match req.accept(Connected::new_empty()).await {
                        Ok(stream) => {
                            let _ = tx.send(stream).await;
                        }
                        Err(e) => {
                            tracing::warn!("StreamRequest::accept falló: {:#}", e);
                        }
                    }
                });
            }
        });

        Ok(HostHandle {
            svc,
            onion,
            incoming: rx,
        })
    }

    /// Dial saliente a `host:port`.
    pub async fn dial(&self, target: &str) -> Result<DataStream> {
        self.client
            .connect(target)
            .await
            .with_context(|| format!("connect a {target}"))
    }
}

pub struct HostHandle {
    pub svc: Arc<RunningOnionService>,
    pub onion: String,
    pub incoming: mpsc::Receiver<DataStream>,
}

fn create_secure_dir(p: &Path) -> Result<()> {
    std::fs::create_dir_all(p)
        .with_context(|| format!("crear {}", p.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("chmod 0700 {}", p.display()))?;
    }
    Ok(())
}
