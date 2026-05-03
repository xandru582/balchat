<script>
  import { tick } from 'svelte'
  import Avatar from './Avatar.svelte'
  import StatusPill from './StatusPill.svelte'
  import MessageBubble from './MessageBubble.svelte'
  import { fmtDayDivider, shortOnion } from '../lib/format.js'

  let {
    selected,
    log = [],
    daemonStatus = 'idle',
    myId,
    handshakeBusy = false,
    onSend,                // (text) => void
    onAttach,              // () => void
    onStartDaemon,         // () => void
    onRefresh,             // () => void
    onLock,                // () => void
    onSettings,            // () => void
    onCopyOnion,           // () => 'ok' | 'err'
    onHandshake,           // (peer) => void
  } = $props()

  let draft = $state('')
  let chatLogEl = $state(null)
  let textareaEl = $state(null)
  let copyState = $state('idle') // 'idle' | 'ok' | 'err'

  // Auto-grow textarea up to 6 lines.
  function autosize() {
    if (!textareaEl) return
    textareaEl.style.height = 'auto'
    const max = 6 * 18 + 18
    textareaEl.style.height = Math.min(textareaEl.scrollHeight, max) + 'px'
  }

  // Auto-scroll to bottom when log or selected changes.
  $effect(() => {
    void log
    void selected
    if (!chatLogEl) return
    tick().then(() => {
      if (chatLogEl) chatLogEl.scrollTop = chatLogEl.scrollHeight
    })
  })

  $effect(() => {
    void draft
    autosize()
  })

  function handleKeydown(e) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      submit()
    }
  }

  function submit() {
    const text = draft.trim()
    if (!text || !selected?.has_group) return
    draft = ''
    onSend?.(text)
    tick().then(autosize)
  }

  async function copyOnion() {
    const r = await onCopyOnion?.()
    copyState = r === 'ok' ? 'ok' : 'err'
    setTimeout(() => (copyState = 'idle'), 1500)
  }

  /** Group messages by day (Hoy / Ayer / fecha). System messages get their own row. */
  let grouped = $derived.by(() => {
    const out = []
    let lastDay = null
    let lastSide = null
    for (let i = 0; i < log.length; i++) {
      const m = log[i]
      const ts = m.created_at
      if (ts) {
        const day = new Date(ts * 1000).toDateString()
        if (day !== lastDay) {
          out.push({ kind: 'divider', label: fmtDayDivider(ts), key: `d-${i}` })
          lastDay = day
          lastSide = null
        }
      }
      const side =
        m.kind === 'sent' ? 'sent' :
        m.kind === 'received' ? 'recv' :
        'system'
      const next = log[i + 1]
      const nextSide = next ? (next.kind === 'sent' ? 'sent' : next.kind === 'received' ? 'recv' : 'system') : null
      const showTail = side !== nextSide || (next && next.created_at && Math.abs(next.created_at - (m.created_at || 0)) > 60)
      out.push({ kind: 'msg', msg: m, showTail, side, key: `m-${i}` })
      lastSide = side
    }
    return out
  })

  let chatTitle = $derived(selected?.label || '')
  let chatSubtitle = $derived(selected ? shortOnion(selected.onion_address) : '')
  let canType = $derived(!!selected?.has_group)
</script>

<section class="chat">
  <header class="topbar" data-tauri-drag-region>
    {#if selected}
      <div class="who no-drag">
        <Avatar label={selected.label} seed={selected.onion_address} size={32} />
        <div class="who-meta">
          <strong>{chatTitle}</strong>
          <small title={selected.onion_address}>
            {chatSubtitle}
            {#if selected.has_group}
              · <span class="ok">conexión segura</span>
            {:else}
              · <span class="warn">sin conectar todavía</span>
            {/if}
          </small>
        </div>
      </div>
    {:else}
      <div class="who placeholder">
        <strong>balchat</strong>
        <small>Selecciona una conversación</small>
      </div>
    {/if}

    <div class="actions no-drag">
      <StatusPill status={daemonStatus} />
      {#if daemonStatus === 'idle' || daemonStatus === 'error'}
        <button class="ghost" onclick={onStartDaemon} title="Conectar a la red Tor">
          Conectar
        </button>
      {/if}
      <button class="icon" onclick={onRefresh} title="Refrescar contactos" aria-label="Refrescar">
        <svg viewBox="0 0 16 16" width="14" height="14">
          <path d="M13 8a5 5 0 1 1-1.46-3.54M13 3v3.5h-3.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" fill="none"/>
        </svg>
      </button>
      <button class="icon" onclick={onSettings} title="Ajustes" aria-label="Ajustes">
        <svg viewBox="0 0 16 16" width="14" height="14">
          <circle cx="8" cy="8" r="2.2" fill="none" stroke="currentColor" stroke-width="1.4"/>
          <path d="M8 1.5v2M8 12.5v2M14.5 8h-2M3.5 8h-2M12.6 3.4l-1.4 1.4M4.8 11.2l-1.4 1.4M12.6 12.6l-1.4-1.4M4.8 4.8L3.4 3.4" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>
        </svg>
      </button>
      <button class="icon" onclick={onLock} title="Cerrar sesión" aria-label="Cerrar sesión">
        <svg viewBox="0 0 16 16" width="14" height="14">
          <rect x="3.5" y="7" width="9" height="6.5" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.4"/>
          <path d="M5.5 7V5a2.5 2.5 0 0 1 5 0v2" stroke="currentColor" stroke-width="1.4" fill="none"/>
        </svg>
      </button>
    </div>
  </header>

  <div class="identity">
    <span class="me-label">Mi código:</span>
    {#if myId?.onion}
      <code title={myId.onion}>{shortOnion(myId.onion)}</code>
      <button class="copy share" onclick={copyOnion} title="Copia tu código para enviárselo a alguien">
        {#if copyState === 'ok'}✓ copiado{:else if copyState === 'err'}× error{:else}Compartir mi código{/if}
      </button>
    {:else if daemonStatus === 'starting'}
      <span class="muted">preparando tu código…</span>
    {:else if daemonStatus === 'running'}
      <span class="muted">preparando tu código…</span>
    {:else}
      <span class="muted">tu código aún no está listo.</span>
      <button class="cta" onclick={onStartDaemon} title="Conectar a la red para generar tu código">
        Conectar a la red
      </button>
    {/if}
  </div>

  {#if selected && !selected.has_group}
    <div class="handshake-banner">
      <div class="hb-text">
        <strong>Aún no estás conectado con {selected.label}</strong>
        <span>Para enviar mensajes hay que establecer una conexión segura. Toca <em>Conectar</em> — necesita que {selected.label} también tenga la app abierta.</span>
      </div>
      <button
        class="hb-btn"
        onclick={() => onHandshake?.(selected.onion_address)}
        disabled={handshakeBusy || daemonStatus !== 'running'}
        title={daemonStatus !== 'running' ? 'Espera a que tu app esté conectada a la red' : 'Establecer conexión segura'}
      >
        {#if handshakeBusy}
          <span class="spinner" aria-hidden="true"></span>
          Conectando…
        {:else}
          Conectar
        {/if}
      </button>
    </div>
  {/if}

  {#if selected}
    <div class="log" bind:this={chatLogEl}>
      {#each grouped as item (item.key)}
        {#if item.kind === 'divider'}
          <div class="divider"><span>{item.label}</span></div>
        {:else}
          <MessageBubble msg={item.msg} showTail={item.showTail} />
        {/if}
      {/each}
      {#if log.length === 0}
        <div class="hint-empty">
          <p>Sin mensajes todavía</p>
          <small>Envía un saludo para empezar la conversación</small>
        </div>
      {/if}
    </div>

    <footer class="composer">
      <button
        class="attach"
        type="button"
        onclick={onAttach}
        disabled={!canType}
        title={canType ? 'Adjuntar archivo' : 'Necesitas conectar primero'}
        aria-label="Adjuntar"
      >
        <svg viewBox="0 0 16 16" width="16" height="16">
          <path d="M9.5 4.5l-4 4a2.5 2.5 0 1 0 3.5 3.5l5-5a4 4 0 0 0-5.7-5.7L3 7" stroke="currentColor" stroke-width="1.4" fill="none" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </button>

      <textarea
        bind:this={textareaEl}
        bind:value={draft}
        onkeydown={handleKeydown}
        placeholder={canType ? 'Mensaje' : 'Toca «Conectar» arriba para poder escribir'}
        rows="1"
        disabled={!canType}
      ></textarea>

      <button
        class="send"
        type="button"
        onclick={submit}
        disabled={!draft.trim() || !canType}
        title="Enviar"
        aria-label="Enviar"
      >
        <svg viewBox="0 0 16 16" width="14" height="14">
          <path d="M8 13V3M8 3l-4 4M8 3l4 4" stroke="currentColor" stroke-width="2" fill="none" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </button>
    </footer>
  {:else}
    <div class="empty">
      <div class="empty-card">
        <Avatar label="bal" seed="balchat" size={64} />
        <h2>balchat</h2>
        <p>Selecciona un contacto en la izquierda para empezar a chatear, o añade uno nuevo con <strong>+</strong>.</p>
      </div>
    </div>
  {/if}
</section>

<style>
  .chat {
    flex: 1;
    min-width: 0;
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    overflow: hidden;
  }

  .topbar {
    height: var(--titlebar-h);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 14px 0 14px;
    border-bottom: 1px solid var(--separator);
    background: var(--bg);
    flex-shrink: 0;
    gap: 12px;
  }
  .who {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .who-meta {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.2;
  }
  .who-meta strong {
    font-size: 13.5px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .who-meta small {
    font-size: 11.5px;
    color: var(--fg-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .who.placeholder {
    flex-direction: column;
    align-items: flex-start;
    gap: 0;
  }
  .who.placeholder strong { font-size: 13.5px; font-weight: 600; }
  .who.placeholder small  { font-size: 11.5px; color: var(--fg-secondary); }

  .ok   { color: var(--success); }
  .warn { color: var(--warning); }

  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .actions .icon {
    width: 26px; height: 26px;
    display: flex; align-items: center; justify-content: center;
    border-radius: 6px;
    color: var(--fg-secondary);
    transition: background 100ms ease, color 100ms ease;
  }
  .actions .icon:hover { background: var(--bg-hover); color: var(--fg); }
  .actions .ghost {
    height: 22px;
    padding: 0 10px;
    border-radius: 6px;
    background: var(--bg-pill);
    color: var(--fg);
    font-size: 11.5px;
    font-weight: 500;
  }
  .actions .ghost:hover { background: var(--border-strong); }

  .identity {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 14px;
    border-bottom: 1px solid var(--separator);
    font-size: 11.5px;
    color: var(--fg-secondary);
    background: var(--bg);
    flex-shrink: 0;
    flex-wrap: wrap;
  }
  .identity code {
    background: var(--bg-pill);
    padding: 1px 6px;
    border-radius: 4px;
    font-size: 11px;
  }
  .identity .copy {
    width: 20px; height: 20px;
    display: inline-flex; align-items: center; justify-content: center;
    border-radius: 4px;
    color: var(--fg-tertiary);
  }
  .identity .copy:hover { background: var(--bg-hover); color: var(--fg); }
  .identity .copy.share {
    width: auto;
    padding: 0 8px;
    height: 20px;
    background: var(--accent);
    color: #fff;
    font-size: 11px;
    font-weight: 600;
    border-radius: 999px;
    letter-spacing: 0.02em;
  }
  .identity .copy.share:hover { background: var(--accent-hover); color: #fff; }
  .identity .me-label { color: var(--fg-tertiary); }
  .identity .sep { color: var(--fg-tertiary); margin: 0 2px; }
  .identity .muted { color: var(--fg-tertiary); font-style: italic; }
  .identity .cta {
    height: 22px;
    padding: 0 10px;
    background: var(--accent);
    color: #fff;
    border-radius: 6px;
    font-size: 11.5px;
    font-weight: 600;
  }
  .identity .cta:hover { background: var(--accent-hover); }

  .log {
    flex: 1;
    overflow-y: auto;
    padding: 18px 22px;
    display: flex;
    flex-direction: column;
    gap: 0;
  }
  .divider {
    text-align: center;
    margin: 14px 0 10px;
    position: relative;
  }
  .divider span {
    background: var(--bg);
    padding: 0 10px;
    font-size: 11px;
    font-weight: 600;
    color: var(--fg-tertiary);
    text-transform: capitalize;
    letter-spacing: 0.04em;
  }
  .hint-empty {
    margin: auto;
    text-align: center;
    color: var(--fg-tertiary);
  }
  .hint-empty p { margin: 0 0 4px; font-size: 13px; color: var(--fg-secondary); }
  .hint-empty small { font-size: 11.5px; }

  .composer {
    display: flex;
    align-items: flex-end;
    gap: 8px;
    padding: 10px 14px 14px;
    border-top: 1px solid var(--separator);
    background: var(--bg);
    flex-shrink: 0;
  }
  .composer textarea {
    flex: 1;
    min-height: 34px;
    max-height: 130px;
    resize: none;
    padding: 8px 12px;
    border-radius: 18px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    font-size: 14px;
    line-height: 1.4;
    overflow-y: auto;
  }
  .composer .attach,
  .composer .send {
    width: 34px; height: 34px;
    flex-shrink: 0;
    border-radius: 50%;
    display: flex; align-items: center; justify-content: center;
    transition: background 100ms ease, color 100ms ease, transform 80ms ease;
  }
  .composer .attach {
    color: var(--fg-secondary);
    background: var(--bg-pill);
  }
  .composer .attach:hover:not(:disabled) {
    background: var(--border-strong);
    color: var(--fg);
  }
  .composer .send {
    color: #fff;
    background: var(--accent);
  }
  .composer .send:hover:not(:disabled) { background: var(--accent-hover); }
  .composer .send:active:not(:disabled) { transform: scale(0.92); }
  .composer .send:disabled { background: var(--bg-pill); color: var(--fg-tertiary); }

  .handshake-banner {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 16px;
    background: color-mix(in srgb, var(--warning) 12%, transparent);
    border-bottom: 1px solid color-mix(in srgb, var(--warning) 30%, transparent);
    flex-shrink: 0;
  }
  .hb-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .hb-text strong { font-size: 12.5px; color: var(--fg); }
  .hb-text span { font-size: 11.5px; color: var(--fg-secondary); line-height: 1.4; }
  .hb-btn {
    flex-shrink: 0;
    height: 28px;
    padding: 0 14px;
    border-radius: 7px;
    background: var(--accent);
    color: #fff;
    font-size: 12px;
    font-weight: 600;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    transition: background 100ms ease;
  }
  .hb-btn:hover:not(:disabled) { background: var(--accent-hover); }
  .spinner {
    width: 11px; height: 11px;
    border-radius: 50%;
    border: 1.6px solid rgba(255, 255, 255, 0.4);
    border-top-color: #fff;
    animation: spin 0.7s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 20px;
  }
  .empty-card {
    text-align: center;
    max-width: 320px;
    color: var(--fg-secondary);
  }
  .empty-card h2 {
    margin: 16px 0 6px;
    font-size: 20px;
    font-weight: 600;
    color: var(--fg);
    letter-spacing: -0.01em;
  }
  .empty-card p { margin: 0; font-size: 13px; line-height: 1.5; }
  .empty-card strong { color: var(--accent); font-weight: 600; }
</style>
