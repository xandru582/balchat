<script>
  import { untrack } from 'svelte'

  let {
    initialRelay = '',
    initialAutoLock = 5,
    daemonStatus = 'idle',
    myId,
    onBack,
    onSaveRelay,
    onSaveAutoLock,
    onPublishKp,
    onExportVault,
  } = $props()

  let relay = $state(untrack(() => initialRelay))
  let autoLock = $state(untrack(() => initialAutoLock))
  let kpCount = $state(10)
  let busy = $state(false)
  let info = $state('')
  let error = $state('')

  async function withBusy(fn, okMsg) {
    busy = true; error = ''; info = ''
    try {
      const r = await fn()
      info = typeof okMsg === 'function' ? okMsg(r) : (okMsg || '')
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }
</script>

<div class="screen">
  <header class="topbar">
    <button class="back" onclick={onBack} aria-label="Volver">
      <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M15 18l-6-6 6-6"/>
      </svg>
    </button>
    <h1>Configuración</h1>
    <span class="title-spacer"></span>
  </header>

  <div class="body">
    {#if myId?.onion}
      <section class="card-info">
        <div class="info-label">Mi código de chat</div>
        <code>{myId.onion}</code>
      </section>
    {/if}

    <section class="group">
      <div class="group-title">Auto-cerrar sesión</div>
      <p class="group-hint">Cierra tu cuenta tras N minutos sin actividad. <code>0</code> desactiva.</p>
      <div class="card">
        <div class="row">
          <span>Minutos</span>
          <input type="number" min="0" max="1440" bind:value={autoLock} disabled={busy} />
        </div>
        <button
          class="action"
          onclick={() => withBusy(
            () => onSaveAutoLock?.(Math.max(0, Math.min(1440, Number(autoLock) | 0))),
            (v) => v === 0 ? 'Auto-cerrar desactivado' : `Auto-cerrar a los ${autoLock} min`
          )}
          disabled={busy}
        >Guardar</button>
      </div>
    </section>

    <section class="group">
      <div class="group-title">Copia de seguridad</div>
      <p class="group-hint">Guarda una copia cifrada de tu cuenta.</p>
      <div class="card">
        <button
          class="action ghost"
          onclick={() => withBusy(async () => {
            const dst = await onExportVault?.()
            return dst
          }, (dst) => dst ? `Cuenta copiada a: ${dst}` : '')}
          disabled={busy}
        >Elegir carpeta…</button>
      </div>
    </section>

    <details class="advanced-block">
      <summary>Opciones avanzadas</summary>

      <section class="group">
        <div class="group-title">Mi buzón offline</div>
        <p class="group-hint">Servidor cifrado donde tus contactos te dejan mensajes. Por defecto el de balchat — no lee tu contenido.</p>
        <div class="card">
          <input
            type="text"
            placeholder="xxx.onion[:1235]"
            bind:value={relay}
            disabled={busy}
            autocapitalize="off"
            autocorrect="off"
          />
          <button
            class="action"
            onclick={() => withBusy(() => onSaveRelay?.(relay.trim()), 'Buzón actualizado')}
            disabled={busy || !relay.trim()}
          >Guardar buzón</button>
        </div>
      </section>

      <section class="group">
        <div class="group-title">Republicar invitaciones</div>
        <p class="group-hint">Sube N invitaciones cifradas al buzón. Se publican automáticamente al arrancar — esto es solo re-fill manual.</p>
        <div class="card">
          <div class="row">
            <span>Cantidad</span>
            <input
              type="number"
              min="1" max="100"
              bind:value={kpCount}
              disabled={busy}
            />
          </div>
          <button
            class="action"
            onclick={() => withBusy(
              () => onPublishKp?.(Math.max(1, Math.min(100, Number(kpCount) | 0))),
              (pool) => `${kpCount} publicadas · ahora hay ${pool}`
            )}
            disabled={busy || daemonStatus !== 'running'}
          >Publicar</button>
        </div>
      </section>
    </details>

    {#if info}<p class="info">{info}</p>{/if}
    {#if error}<p class="err">{error}</p>{/if}
  </div>
</div>

<style>
  .screen {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    overflow: hidden;
    padding-top: var(--safe-top);
  }
  .topbar {
    display: flex;
    align-items: center;
    padding: 6px 8px;
    border-bottom: 1px solid var(--separator);
    flex-shrink: 0;
  }
  .topbar h1 {
    flex: 1;
    margin: 0;
    text-align: center;
    font-size: 16px;
    font-weight: 600;
  }
  .back {
    width: var(--m-touch);
    height: var(--m-touch);
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    color: var(--accent);
  }
  .back:active { background: var(--bg-hover); }
  .title-spacer { width: var(--m-touch); height: var(--m-touch); }

  .body {
    flex: 1;
    overflow-y: auto;
    padding: 14px 16px calc(40px + var(--safe-bottom));
  }
  .card-info {
    padding: 14px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: 12px;
    margin-bottom: 18px;
  }
  .info-label {
    font-size: 11px;
    color: var(--fg-tertiary);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-weight: 600;
    margin-bottom: 4px;
  }
  .card-info code {
    font-size: 13px;
    background: transparent;
    padding: 0;
    color: var(--fg);
    word-break: break-all;
  }

  .group { margin-bottom: 22px; }
  .group-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--fg-tertiary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-bottom: 6px;
    padding: 0 4px;
  }
  .group-hint {
    margin: 0 4px 8px;
    font-size: 12px;
    color: var(--fg-secondary);
    line-height: 1.4;
  }
  .card {
    padding: 8px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .card input {
    height: 40px;
    padding: 0 12px;
    border-radius: 8px;
    flex: 1;
    min-width: 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 8px;
  }
  .row span { font-size: 14px; color: var(--fg); }

  .action {
    height: 40px;
    border-radius: 8px;
    background: var(--accent);
    color: #fff;
    font-weight: 600;
    font-size: 14px;
  }
  .action:disabled { opacity: 0.5; }
  .action:active:not(:disabled) { background: var(--accent-hover); }
  .action.ghost {
    background: var(--bg-pill);
    color: var(--fg);
  }

  .info {
    margin: 12px 0 0;
    padding: 10px 12px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--success) 14%, transparent);
    color: var(--success);
    font-size: 13px;
  }
  .err {
    margin: 12px 0 0;
    padding: 10px 12px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--danger) 14%, transparent);
    color: var(--danger);
    font-size: 13px;
  }
  .advanced-block {
    margin-top: 12px;
    border-top: 1px solid var(--separator);
    padding-top: 14px;
  }
  .advanced-block > summary {
    cursor: pointer;
    color: var(--accent);
    font-size: 13.5px;
    font-weight: 500;
    padding: 6px 4px;
    list-style: none;
    user-select: none;
  }
  .advanced-block > summary::-webkit-details-marker { display: none; }
  .advanced-block > summary::before {
    content: '▸';
    display: inline-block;
    margin-right: 8px;
    transition: transform 150ms ease;
  }
  .advanced-block[open] > summary::before { transform: rotate(90deg); }
  .advanced-block .group { margin-top: 18px; }
</style>
