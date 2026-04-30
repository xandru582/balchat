//! balchat — Fase 0 spike: arti + onion service v3 end-to-end.
//!
//! Demuestra:
//!   1. Bootstrap de un TorClient embebido (sin daemon `tor` externo).
//!   2. Lanzamiento de un onion service v3 propio.
//!   3. Aceptación de conexiones entrantes (echo handler).
//!   4. Conexión saliente al propio .onion vía la red Tor real.
//!
//! Tarda ~2 min la primera vez (bootstrap + publicación HS).

use anyhow::{anyhow, Context, Result};
use arti_client::{TorClient, TorClientConfig};
use futures::StreamExt;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use safelog::DisplayRedacted;
use tor_hsservice::config::OnionServiceConfig;
use tor_hsservice::{handle_rend_requests, HsNickname};

const VIRTUAL_PORT: u16 = 1234;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,arti=warn,tor_=warn")),
        )
        .init();

    println!("=== balchat — spike Tor ===\n");

    // -------- 1. Bootstrap del cliente Tor (puro Rust, sin daemon externo) --------
    println!("[1] Bootstrapping Arti (puede tardar 1-3 min la primera vez)...");
    let client = TorClient::create_bootstrapped(TorClientConfig::default())
        .await
        .context("bootstrap arti")?;
    println!("    Arti listo.");

    // -------- 2. Levantar onion service v3 --------
    let nickname: HsNickname = "balchat-spike".parse().context("HsNickname")?;
    let svc_cfg = OnionServiceConfig::builder()
        .nickname(nickname)
        .build()
        .context("OnionServiceConfig::build")?;

    println!("[2] Lanzando onion service v3...");
    let (svc, rend_stream) = client
        .launch_onion_service(svc_cfg)
        .context("launch_onion_service")?
        .ok_or_else(|| anyhow!("onion service deshabilitado en config"))?;

    let onion_id = svc
        .onion_address()
        .ok_or_else(|| anyhow!("RunningOnionService no devolvió dirección .onion"))?;
    let onion_str = onion_id.display_unredacted().to_string();
    println!("    .onion: {}", onion_str);

    // -------- 3. Atender conexiones entrantes (echo) --------
    let server = tokio::spawn(async move {
        let mut requests = std::pin::pin!(handle_rend_requests(rend_stream));
        while let Some(req) = requests.next().await {
            tokio::spawn(async move {
                if let Err(e) = handle_stream_request(req).await {
                    eprintln!("    [server] error en conexión: {:#}", e);
                }
            });
        }
    });

    // -------- 4. Esperar publicación HS y conectar como cliente al propio .onion --------
    println!("[3] Esperando publicación del descriptor HS (60s)...");
    tokio::time::sleep(Duration::from_secs(60)).await;

    println!("[4] Conectando al .onion como cliente...");
    let target = format!("{}:{}", onion_str, VIRTUAL_PORT);
    let mut stream = {
        let mut last_err = None;
        let attempts = 5u32;
        let mut connected = None;
        for i in 1..=attempts {
            match client.connect(target.as_str()).await {
                Ok(s) => {
                    connected = Some(s);
                    break;
                }
                Err(e) => {
                    eprintln!("    intento {}/{} falló: {:#}", i, attempts, e);
                    last_err = Some(e);
                    tokio::time::sleep(Duration::from_secs(15)).await;
                }
            }
        }
        connected.ok_or_else(|| {
            anyhow!(
                "no se pudo conectar a {} tras {} intentos: {:?}",
                target,
                attempts,
                last_err
            )
        })?
    };

    let msg = b"ping desde balchat\n";
    stream.write_all(msg).await?;
    stream.flush().await?;
    println!("[5] Cliente envía: {:?}", String::from_utf8_lossy(msg).trim());

    let mut reader = BufReader::new(&mut stream);
    let mut response = String::new();
    reader.read_line(&mut response).await?;
    println!("[6] Cliente recibe: {:?}", response.trim());

    println!("\n[OK] Onion service v3 reachable end-to-end vía Arti.");
    drop(server);
    Ok(())
}

async fn handle_stream_request(req: tor_hsservice::StreamRequest) -> Result<()> {
    use tor_cell::relaycell::msg::Connected;

    let stream = req.accept(Connected::new_empty()).await.context("accept")?;
    let (reader, mut writer) = tokio::io::split(stream);
    let mut buf_reader = BufReader::new(reader);

    let mut line = String::new();
    buf_reader.read_line(&mut line).await.context("read_line")?;
    let trimmed = line.trim_end_matches('\n');
    println!("    [server] recibido: {:?}", trimmed);

    let response = format!("echo: {}\n", trimmed);
    writer.write_all(response.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}
