<script>
  import { untrack } from 'svelte'

  let {
    initialRelay = '',
    initialAutoLock = 5,
    daemonStatus = 'idle',
    onSaveRelay,            // (relay) => Promise
    onSaveAutoLock,         // (minutes) => Promise
    onPublishKp,            // (count) => Promise<number>
    onExportVault,          // () => Promise<string|null>
    onClose,
  } = $props()

  // Snapshot the initial values so user edits aren't clobbered by reactive prop reads.
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

  function backdropClick(e) {
    if (e.target === e.currentTarget) onClose?.()
  }
  function onKeydown(e) {
    if (e.key === 'Escape') onClose?.()
  }
</script>

<div
  class="backdrop"
  onclick={backdropClick}
  onkeydown={onKeydown}
  role="presentation"
>
  <div class="sheet" role="dialog" aria-label="Configuración" tabindex="-1">
    <header data-tauri-drag-region>
      <div class="title-spacer"></div>
      <h2>Configuración</h2>
      <button class="close no-drag" onclick={onClose} aria-label="Cerrar" title="Cerrar">
        <svg viewBox="0 0 16 16" width="13" height="13"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>
      </button>
    </header>

    <div class="body">
      <section>
        <h3>Auto-cerrar sesión</h3>
        <p>Cierra tu cuenta tras N minutos sin actividad. <code>0</code> desactiva.</p>
        <div class="row">
          <input
            type="number"
            min="0" max="1440"
            bind:value={autoLock}
            disabled={busy}
            style="max-width: 80px"
          />
          <span class="suffix">minutos</span>
          <button
            class="primary"
            onclick={() => withBusy(
              () => onSaveAutoLock?.(Math.max(0, Math.min(1440, Number(autoLock) | 0))),
              (v) => v === 0 ? 'Auto-lock desactivado' : `Auto-lock = ${autoLock} min`
            )}
            disabled={busy}
          >Guardar</button>
        </div>
      </section>

      <section>
        <h3>Copia de seguridad</h3>
        <p>Guarda una copia de tu cuenta cifrada en una carpeta. Mantiene tu contraseña original.</p>
        <button
          class="secondary"
          onclick={() => withBusy(async () => {
            const dst = await onExportVault?.()
            if (!dst) { info = ''; return null }
            return dst
          }, (dst) => dst ? `Cuenta copiada a: ${dst}` : '')}
          disabled={busy}
        >Elegir carpeta…</button>
      </section>

      <details class="advanced">
        <summary>Opciones avanzadas</summary>
        <section>
          <h3>Mi buzón offline</h3>
          <p>Servidor cifrado donde tus contactos te dejan mensajes cuando no estás conectado. Por defecto usamos el buzón público de balchat — no puede leer tus mensajes (van cifrados de extremo a extremo). Solo cámbialo si tienes uno propio.</p>
          <div class="row">
            <input
              type="text"
              placeholder="xxxxxx.onion[:1235]"
              bind:value={relay}
              disabled={busy}
              autocapitalize="off"
            />
            <button
              class="primary"
              onclick={() => withBusy(() => onSaveRelay?.(relay.trim()), 'Buzón actualizado')}
              disabled={busy || !relay.trim()}
            >Guardar</button>
          </div>
        </section>

        <section>
          <h3>Republicar invitaciones (KeyPackages)</h3>
          <p>Sube N invitaciones cifradas a tu buzón para que contactos offline puedan iniciar conexión contigo. Se publican automáticamente al arrancar — esto es un re-fill manual.</p>
          <div class="row">
            <input
              type="number"
              min="1" max="100"
              bind:value={kpCount}
              disabled={busy}
              style="max-width: 80px"
            />
            <button
              class="primary"
              onclick={() => withBusy(
                () => onPublishKp?.(Math.max(1, Math.min(100, Number(kpCount) | 0))),
                (pool) => `${kpCount} publicadas · ahora hay ${pool} disponibles`
              )}
              disabled={busy || daemonStatus !== 'running'}
              title={daemonStatus !== 'running' ? 'Espera a estar conectado' : ''}
            >Publicar</button>
          </div>
        </section>
      </details>

      {#if info}<p class="info">{info}</p>{/if}
      {#if error}<p class="err">{error}</p>{/if}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: var(--bg-modal-backdrop);
    backdrop-filter: blur(4px);
    -webkit-backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 50;
    padding: 24px;
  }
  .sheet {
    width: 460px;
    max-width: 100%;
    max-height: calc(100vh - 48px);
    background: var(--bg-modal);
    border: 1px solid var(--border);
    border-radius: 14px;
    box-shadow: var(--shadow-lg);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    outline: none;
  }
  header {
    height: 36px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 10px;
    border-bottom: 1px solid var(--separator);
    background: var(--bg-modal);
    flex-shrink: 0;
  }
  header h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--fg);
    flex: 1;
    text-align: center;
  }
  .title-spacer { width: 22px; }
  .close {
    width: 22px; height: 22px;
    display: flex; align-items: center; justify-content: center;
    border-radius: 50%;
    color: var(--fg-secondary);
    background: transparent;
  }
  .close:hover { background: var(--bg-hover); color: var(--fg); }

  .body {
    padding: 18px 20px 16px;
    overflow-y: auto;
  }
  section { margin-bottom: 18px; }
  section h3 {
    margin: 0 0 4px;
    font-size: 13px;
    font-weight: 600;
    color: var(--fg);
  }
  section p {
    margin: 0 0 10px;
    font-size: 12px;
    color: var(--fg-secondary);
    line-height: 1.45;
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .row input {
    flex: 1;
    min-width: 0;
    padding: 7px 10px;
    font-size: 13px;
  }
  .suffix { font-size: 12px; color: var(--fg-secondary); }

  .primary {
    padding: 7px 14px;
    background: var(--accent);
    color: #fff;
    border-radius: 7px;
    font-weight: 600;
    font-size: 12.5px;
  }
  .primary:hover:not(:disabled) { background: var(--accent-hover); }
  .secondary {
    padding: 7px 14px;
    background: var(--bg-pill);
    color: var(--fg);
    border-radius: 7px;
    font-weight: 500;
    font-size: 12.5px;
  }
  .secondary:hover:not(:disabled) { background: var(--border-strong); }

  .info {
    margin: 4px 0 0;
    padding: 8px 10px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--success) 14%, transparent);
    color: var(--success);
    font-size: 12px;
  }
  .err {
    margin: 4px 0 0;
    padding: 8px 10px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--danger) 14%, transparent);
    color: var(--danger);
    font-size: 12px;
  }
  .advanced {
    margin-top: 8px;
    border-top: 1px solid var(--separator);
    padding-top: 12px;
  }
  .advanced > summary {
    cursor: pointer;
    color: var(--accent);
    font-size: 12.5px;
    font-weight: 500;
    padding: 4px 0;
    list-style: none;
    user-select: none;
  }
  .advanced > summary::-webkit-details-marker { display: none; }
  .advanced > summary::before {
    content: '▸';
    display: inline-block;
    margin-right: 6px;
    transition: transform 150ms ease;
  }
  .advanced[open] > summary::before { transform: rotate(90deg); }
  .advanced section { margin-top: 14px; }
</style>
