<script>
  import Avatar from '../Avatar.svelte'
  import StatusPill from '../StatusPill.svelte'
  import { fmtSidebarTime, previewText, shortOnion } from '../../lib/format.js'

  let {
    contacts = [],
    daemonStatus = 'idle',
    myId,
    onOpenChat,
    onAddContact,
    onSettings,
    onLock,
    onCopyOnion,
    onStartDaemon,
  } = $props()

  let query = $state('')
  let copyState = $state('idle')

  let filtered = $derived.by(() => {
    const q = query.trim().toLowerCase()
    if (!q) return contacts
    return contacts.filter((c) =>
      (c.label || '').toLowerCase().includes(q) ||
      (c.onion_address || '').toLowerCase().includes(q)
    )
  })

  async function copy() {
    const r = await onCopyOnion?.()
    copyState = r === 'ok' ? 'ok' : 'err'
    setTimeout(() => (copyState = 'idle'), 1500)
  }
</script>

<div class="screen">
  <header class="topbar">
    <div class="title">
      <h1>balchat</h1>
      <div class="sub">
        <StatusPill status={daemonStatus} />
      </div>
    </div>
    <div class="head-actions">
      <button class="icon" onclick={onSettings} aria-label="Configuración" title="Configuración">
        <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
          <circle cx="12" cy="12" r="3"/>
          <path d="M12 2v3M12 19v3M22 12h-3M5 12H2M19.07 4.93l-2.12 2.12M7.05 16.95l-2.12 2.12M19.07 19.07l-2.12-2.12M7.05 7.05L4.93 4.93"/>
        </svg>
      </button>
      <button class="icon" onclick={onLock} aria-label="Cerrar sesión" title="Cerrar sesión">
        <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.8">
          <rect x="5" y="11" width="14" height="10" rx="2"/>
          <path d="M8 11V8a4 4 0 0 1 8 0v3"/>
        </svg>
      </button>
    </div>
  </header>

  <div class="me-card">
    <div class="me-row">
      <div class="me-text">
        <div class="me-label">Mi código</div>
        {#if myId?.onion}
          <code title={myId.onion}>{shortOnion(myId.onion)}</code>
        {:else if daemonStatus === 'starting' || daemonStatus === 'running'}
          <span class="muted">preparando tu código…</span>
        {:else}
          <span class="muted">tu código aún no está listo</span>
        {/if}
      </div>
      {#if myId?.onion}
        <button class="share-btn" onclick={copy}>
          {copyState === 'ok' ? '✓ Copiado' : copyState === 'err' ? '× Error' : 'Compartir'}
        </button>
      {:else if daemonStatus !== 'starting' && daemonStatus !== 'running'}
        <button class="share-btn" onclick={onStartDaemon}>Conectar</button>
      {/if}
    </div>
  </div>

  <div class="search">
    <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
      <circle cx="7" cy="7" r="4.5" fill="none" stroke="currentColor" stroke-width="1.5"/>
      <path d="M10.5 10.5L13 13" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
    </svg>
    <input type="text" placeholder="Buscar contactos" bind:value={query} />
  </div>

  <div class="list">
    {#if filtered.length === 0}
      <div class="empty">
        {#if contacts.length === 0}
          <p>Sin contactos todavía</p>
          <small>Toca el botón <strong>+</strong> para añadir el primero</small>
        {:else}
          <p>Nada coincide con «{query}»</p>
        {/if}
      </div>
    {:else}
      {#each filtered as c (c.onion_address)}
        <button
          class="row"
          type="button"
          onclick={() => onOpenChat?.(c)}
        >
          <Avatar label={c.label} seed={c.onion_address} size={48} />
          <div class="meta">
            <div class="top">
              <span class="name">{c.label}</span>
              {#if c.last_created_at}
                <time>{fmtSidebarTime(c.last_created_at)}</time>
              {/if}
            </div>
            <div class="bottom">
              <span class="preview">{previewText(c) || c.onion_address}</span>
              {#if c.unread_count > 0}
                <span class="unread">{c.unread_count > 99 ? '99+' : c.unread_count}</span>
              {:else if !c.has_group}
                <span class="warn-dot" title="Sin conexión segura todavía"></span>
              {/if}
            </div>
          </div>
        </button>
      {/each}
    {/if}
  </div>

  <button class="fab" onclick={onAddContact} aria-label="Añadir contacto" title="Añadir contacto">
    <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
      <path d="M12 5v14M5 12h14"/>
    </svg>
  </button>
</div>

<style>
  .screen {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    overflow: hidden;
    padding-top: var(--safe-top);
    padding-bottom: var(--safe-bottom);
    position: relative;
  }
  .topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px 8px;
    flex-shrink: 0;
  }
  .title { display: flex; flex-direction: column; gap: 4px; }
  .title h1 {
    margin: 0;
    font-size: 26px;
    font-weight: 700;
    letter-spacing: -0.02em;
  }
  .sub { display: flex; align-items: center; gap: 8px; }
  .head-actions { display: flex; gap: 4px; }
  .icon {
    width: var(--m-touch);
    height: var(--m-touch);
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    color: var(--fg-secondary);
  }
  .icon:active { background: var(--bg-hover); }

  .me-card {
    margin: 4px 16px 12px;
    padding: 10px 14px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: 14px;
  }
  .me-row { display: flex; align-items: center; gap: 12px; }
  .me-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .me-label { font-size: 11px; color: var(--fg-tertiary); text-transform: uppercase; letter-spacing: 0.04em; font-weight: 600; }
  .me-text code {
    font-size: 13px;
    background: transparent;
    padding: 0;
    color: var(--fg);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .me-text .muted { font-size: 13px; color: var(--fg-tertiary); font-style: italic; }
  .share-btn {
    height: 32px;
    padding: 0 14px;
    border-radius: 999px;
    background: var(--accent);
    color: #fff;
    font-size: 13px;
    font-weight: 600;
    flex-shrink: 0;
  }
  .share-btn:active { background: var(--accent-hover); }

  .search {
    margin: 0 16px 8px;
    position: relative;
    display: flex;
    align-items: center;
  }
  .search svg {
    position: absolute;
    left: 12px;
    color: var(--fg-tertiary);
    pointer-events: none;
  }
  .search input {
    width: 100%;
    height: 38px;
    padding: 0 14px 0 34px;
    border-radius: 10px;
    background: var(--bg-pill);
    border: 1px solid transparent;
  }
  .search input:focus { background: var(--bg-input); }

  .list {
    flex: 1;
    overflow-y: auto;
    padding: 4px 8px 80px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 12px;
    border-radius: 12px;
    background: transparent;
    color: inherit;
    text-align: left;
  }
  .row:active { background: var(--bg-hover); }
  .meta { flex: 1; min-width: 0; }
  .top {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }
  .name {
    font-weight: 600;
    font-size: 15.5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .top time {
    font-size: 12px;
    color: var(--fg-tertiary);
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
  }
  .bottom {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 2px;
  }
  .preview {
    flex: 1;
    min-width: 0;
    font-size: 13.5px;
    color: var(--fg-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .unread {
    background: var(--accent);
    color: #fff;
    font-size: 11.5px;
    font-weight: 700;
    padding: 1px 8px;
    border-radius: 999px;
    line-height: 1.5;
    min-width: 22px;
    text-align: center;
  }
  .warn-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--warning);
    flex-shrink: 0;
  }

  .empty {
    text-align: center;
    color: var(--fg-tertiary);
    padding: 60px 20px;
  }
  .empty p { margin: 0 0 6px; font-size: 14px; color: var(--fg-secondary); }
  .empty small { font-size: 12.5px; }
  .empty strong { color: var(--accent); font-weight: 600; }

  .fab {
    position: absolute;
    right: calc(20px + var(--safe-right));
    bottom: calc(24px + var(--safe-bottom));
    width: 56px;
    height: 56px;
    border-radius: 50%;
    background: var(--accent);
    color: #fff;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 8px 24px color-mix(in srgb, var(--accent) 45%, transparent);
    z-index: 10;
  }
  .fab:active { transform: scale(0.94); }
</style>
