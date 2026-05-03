<script>
  import { tick } from 'svelte'
  import Avatar from '../Avatar.svelte'
  import MessageBubble from '../MessageBubble.svelte'
  import { fmtDayDivider, shortOnion } from '../../lib/format.js'

  let {
    contact,
    log = [],
    daemonStatus = 'idle',
    handshakeBusy = false,
    onBack,
    onSend,
    onAttach,
    onHandshake,
    onEdit,
  } = $props()

  let draft = $state('')
  let chatLogEl = $state(null)
  let textareaEl = $state(null)

  function autosize() {
    if (!textareaEl) return
    textareaEl.style.height = 'auto'
    textareaEl.style.height = Math.min(textareaEl.scrollHeight, 130) + 'px'
  }

  $effect(() => {
    void log
    if (!chatLogEl) return
    tick().then(() => { if (chatLogEl) chatLogEl.scrollTop = chatLogEl.scrollHeight })
  })
  $effect(() => { void draft; autosize() })

  function submit() {
    const text = draft.trim()
    if (!text || !contact?.has_group) return
    draft = ''
    onSend?.(text)
    tick().then(autosize)
  }

  let grouped = $derived.by(() => {
    const out = []
    let lastDay = null
    for (let i = 0; i < log.length; i++) {
      const m = log[i]
      const ts = m.created_at
      if (ts) {
        const day = new Date(ts * 1000).toDateString()
        if (day !== lastDay) {
          out.push({ kind: 'divider', label: fmtDayDivider(ts), key: `d-${i}` })
          lastDay = day
        }
      }
      const side = m.kind === 'sent' ? 'sent' : m.kind === 'received' ? 'recv' : 'system'
      const next = log[i + 1]
      const nextSide = next ? (next.kind === 'sent' ? 'sent' : next.kind === 'received' ? 'recv' : 'system') : null
      const showTail = side !== nextSide || (next && next.created_at && Math.abs(next.created_at - (m.created_at || 0)) > 60)
      out.push({ kind: 'msg', msg: m, showTail, key: `m-${i}` })
    }
    return out
  })

  let canType = $derived(!!contact?.has_group)
</script>

<div class="screen">
  <header class="topbar">
    <button class="back" onclick={onBack} aria-label="Atrás">
      <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M15 18l-6-6 6-6"/>
      </svg>
    </button>
    <div class="who">
      <Avatar label={contact?.label} seed={contact?.onion_address} size={32} />
      <div class="who-meta">
        <strong>{contact?.label || ''}</strong>
        <small>
          {shortOnion(contact?.onion_address || '')}
          {#if contact?.has_group}· <span class="ok">conexión segura</span>{:else}· <span class="warn">sin conectar</span>{/if}
        </small>
      </div>
    </div>
    <button class="edit-btn" onclick={() => onEdit?.(contact)} aria-label="Editar contacto" title="Editar">
      <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="5" cy="12" r="1.6"/>
        <circle cx="12" cy="12" r="1.6"/>
        <circle cx="19" cy="12" r="1.6"/>
      </svg>
    </button>
  </header>

  {#if contact && !contact.has_group}
    <div class="handshake-banner">
      <div class="hb-text">
        <strong>Aún no estás conectado</strong>
        <span>Toca <em>Conectar</em> para establecer una conexión segura con {contact.label}</span>
      </div>
      <button
        class="hb-btn"
        onclick={() => onHandshake?.(contact.onion_address)}
        disabled={handshakeBusy || daemonStatus !== 'running'}
      >
        {#if handshakeBusy}
          <span class="spinner" aria-hidden="true"></span> Conectando…
        {:else}
          Conectar
        {/if}
      </button>
    </div>
  {/if}

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
        <small>Envía un saludo para empezar</small>
      </div>
    {/if}
  </div>

  <footer class="composer">
    <button
      class="attach"
      type="button"
      onclick={onAttach}
      disabled={!canType}
      aria-label="Adjuntar"
    >
      <svg viewBox="0 0 16 16" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
        <path d="M9.5 4.5l-4 4a2.5 2.5 0 1 0 3.5 3.5l5-5a4 4 0 0 0-5.7-5.7L3 7"/>
      </svg>
    </button>
    <textarea
      bind:this={textareaEl}
      bind:value={draft}
      placeholder={canType ? 'Mensaje' : 'Toca «Conectar» arriba para escribir'}
      rows="1"
      disabled={!canType}
    ></textarea>
    <button
      class="send"
      type="button"
      onclick={submit}
      disabled={!draft.trim() || !canType}
      aria-label="Enviar"
    >
      <svg viewBox="0 0 16 16" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M8 13V3M8 3l-4 4M8 3l4 4"/>
      </svg>
    </button>
  </footer>
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
    gap: 8px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--separator);
    background: var(--bg);
    flex-shrink: 0;
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
  .edit-btn {
    width: var(--m-touch);
    height: var(--m-touch);
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    color: var(--fg-secondary);
    flex-shrink: 0;
  }
  .edit-btn:active { background: var(--bg-hover); color: var(--fg); }
  .who {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .who-meta { display: flex; flex-direction: column; min-width: 0; line-height: 1.2; }
  .who-meta strong {
    font-size: 15px;
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
  .ok { color: var(--success); }
  .warn { color: var(--warning); }

  .handshake-banner {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    background: color-mix(in srgb, var(--warning) 14%, transparent);
    border-bottom: 1px solid color-mix(in srgb, var(--warning) 28%, transparent);
    flex-shrink: 0;
  }
  .hb-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .hb-text strong { font-size: 13px; }
  .hb-text span { font-size: 12px; color: var(--fg-secondary); }
  .hb-btn {
    height: 32px;
    padding: 0 14px;
    border-radius: 8px;
    background: var(--accent);
    color: #fff;
    font-size: 13px;
    font-weight: 600;
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .hb-btn:active:not(:disabled) { background: var(--accent-hover); }
  .spinner {
    width: 12px; height: 12px;
    border-radius: 50%;
    border: 1.6px solid rgba(255, 255, 255, 0.4);
    border-top-color: #fff;
    animation: spin 0.7s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .log {
    flex: 1;
    overflow-y: auto;
    padding: 12px 14px;
  }
  .divider {
    text-align: center;
    margin: 12px 0;
  }
  .divider span {
    background: var(--bg);
    padding: 0 10px;
    font-size: 11px;
    font-weight: 600;
    color: var(--fg-tertiary);
    text-transform: capitalize;
  }
  .hint-empty {
    margin: auto;
    text-align: center;
    color: var(--fg-tertiary);
    padding: 40px 20px;
  }
  .hint-empty p { margin: 0 0 4px; color: var(--fg-secondary); }
  .hint-empty small { font-size: 12.5px; }

  .composer {
    display: flex;
    align-items: flex-end;
    gap: 8px;
    padding: 8px 10px calc(10px + var(--safe-bottom));
    border-top: 1px solid var(--separator);
    background: var(--bg);
    flex-shrink: 0;
  }
  .composer textarea {
    flex: 1;
    min-height: 38px;
    max-height: 130px;
    resize: none;
    padding: 9px 14px;
    border-radius: 22px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    line-height: 1.4;
  }
  .composer .attach,
  .composer .send {
    width: 38px;
    height: 38px;
    border-radius: 50%;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .composer .attach {
    background: var(--bg-pill);
    color: var(--fg-secondary);
  }
  .composer .send {
    background: var(--accent);
    color: #fff;
  }
  .composer .send:disabled { background: var(--bg-pill); color: var(--fg-tertiary); }
  .composer .send:active:not(:disabled) { transform: scale(0.92); }
</style>
