<script>
  let { onBack, onSave } = $props()

  let form = $state({ label: '', onion: '', relay: '', queue: '', pubkey: '' })
  let busy = $state(false)
  let error = $state('')
  let advanced = $state(false)

  async function submit() {
    error = ''
    if (!form.label.trim()) { error = 'Nombre obligatorio'; return }
    if (!form.onion.trim()) { error = 'Código de chat obligatorio'; return }
    busy = true
    try {
      await onSave?.({
        label: form.label.trim(),
        onion: form.onion.trim(),
        relay: form.relay.trim() || null,
        queueHex: form.queue.trim() || null,
        pubkeyHex: form.pubkey.trim() || null,
      })
      onBack?.()
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }
</script>

<div class="screen">
  <header class="topbar">
    <button class="back" onclick={onBack} aria-label="Cancelar">
      <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
        <path d="M6 6l12 12M18 6L6 18"/>
      </svg>
    </button>
    <h1>Nuevo contacto</h1>
    <button
      class="save"
      onclick={submit}
      disabled={busy || !form.label.trim() || !form.onion.trim()}
    >{busy ? 'Guardando…' : 'Guardar'}</button>
  </header>

  <form class="body" onsubmit={(e) => { e.preventDefault(); submit() }}>
    <label class="field">
      <span>Nombre</span>
      <input type="text" placeholder="Alice" bind:value={form.label} disabled={busy} />
    </label>

    <label class="field">
      <span>Código de chat</span>
      <input type="text" placeholder="Pega aquí el código que te pasaron" bind:value={form.onion} disabled={busy} autocapitalize="off" autocorrect="off" />
    </label>

    <button
      type="button"
      class="adv-toggle"
      onclick={() => (advanced = !advanced)}
    >
      <span>Opciones avanzadas</span>
      <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" style="transform: rotate({advanced ? 180 : 0}deg); transition: transform 200ms ease;">
        <path d="M4 6l4 4 4-4"/>
      </svg>
    </button>

    {#if advanced}
      <label class="field">
        <span>Buzón offline propio <em>opcional</em></span>
        <input type="text" placeholder="xxx.onion[:1235]" bind:value={form.relay} disabled={busy} autocapitalize="off" autocorrect="off" />
      </label>

      <label class="field">
        <span>Queue ID <em>64 hex, opcional</em></span>
        <input type="text" placeholder="abcd…" bind:value={form.queue} disabled={busy} autocapitalize="off" autocorrect="off" />
      </label>

      <label class="field">
        <span>Pubkey hex <em>opcional</em></span>
        <input type="text" placeholder="…" bind:value={form.pubkey} disabled={busy} autocapitalize="off" autocorrect="off" />
      </label>
    {/if}

    {#if error}
      <p class="error">{error}</p>
    {/if}
  </form>
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
    padding: 6px 10px;
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
  .back, .save {
    height: var(--m-touch);
    border-radius: 50%;
  }
  .back {
    width: var(--m-touch);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--accent);
  }
  .back:active { background: var(--bg-hover); }
  .save {
    padding: 0 14px;
    color: var(--accent);
    font-weight: 600;
    font-size: 15px;
  }
  .save:disabled { opacity: 0.4; }

  .body {
    flex: 1;
    overflow-y: auto;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding-bottom: calc(20px + var(--safe-bottom));
  }
  .field { display: flex; flex-direction: column; gap: 6px; }
  .field > span {
    font-size: 12px;
    font-weight: 500;
    color: var(--fg-secondary);
  }
  .field em {
    font-style: normal;
    color: var(--fg-tertiary);
    margin-left: 4px;
  }
  .field input {
    height: 44px;
    padding: 0 14px;
    border-radius: 10px;
    background: var(--bg-elevated);
  }
  .adv-toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 0;
    color: var(--accent);
    font-size: 13.5px;
    font-weight: 500;
  }
  .error {
    margin: 8px 0 0;
    padding: 10px 12px;
    background: color-mix(in srgb, var(--danger) 14%, transparent);
    color: var(--danger);
    border-radius: 8px;
    font-size: 13px;
  }
</style>
