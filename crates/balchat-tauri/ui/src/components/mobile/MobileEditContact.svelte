<script>
  import { untrack } from 'svelte'

  let { contact, onBack, onSave, onDelete } = $props()

  let label = $state(untrack(() => contact?.label || ''))
  let relay = $state(untrack(() => contact?.relay_onion || ''))
  // Queue/pubkey hex are write-only (we don't pre-fill — leaving empty preserves the existing value).
  let queue = $state('')
  let pubkey = $state('')
  let busy = $state(false)
  let error = $state('')
  let advanced = $state(false)

  async function submit() {
    error = ''
    if (!label.trim()) { error = 'Nombre obligatorio'; return }
    busy = true
    try {
      await onSave?.({
        peer: contact.onion_address,
        label: label.trim(),
        relay: relay.trim(),
        queueHex: queue.trim(),
        pubkeyHex: pubkey.trim(),
      })
      onBack?.()
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }

  async function confirmDelete() {
    const ok = window.confirm(`¿Borrar a "${contact.label}" y todo su historial? Esta acción no se puede deshacer.`)
    if (!ok) return
    busy = true
    try {
      await onDelete?.(contact)
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
    <h1>Editar contacto</h1>
    <button
      class="save"
      onclick={submit}
      disabled={busy || !label.trim()}
    >{busy ? '…' : 'Guardar'}</button>
  </header>

  <form class="body" onsubmit={(e) => { e.preventDefault(); submit() }}>
    <label class="field">
      <span>Nombre</span>
      <input type="text" bind:value={label} disabled={busy} autocapitalize="words" />
    </label>

    <label class="field">
      <span>Código de chat <em>no se puede cambiar</em></span>
      <input type="text" value={contact?.onion_address || ''} disabled readonly class="readonly" />
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
        <span>Buzón offline <em>vacío para usar el público</em></span>
        <input type="text" placeholder="xxx.onion[:1235]" bind:value={relay} disabled={busy} autocapitalize="off" autocorrect="off" />
      </label>

      <label class="field">
        <span>Sobrescribir queue ID <em>opcional</em></span>
        <input type="text" placeholder="64-hex" bind:value={queue} disabled={busy} autocapitalize="off" autocorrect="off" />
      </label>

      <label class="field">
        <span>Sobrescribir pubkey <em>opcional</em></span>
        <input type="text" placeholder="…" bind:value={pubkey} disabled={busy} autocapitalize="off" autocorrect="off" />
      </label>
    {/if}

    {#if error}
      <p class="error">{error}</p>
    {/if}

    <button type="button" class="danger" onclick={confirmDelete} disabled={busy}>
      Borrar contacto y todo el historial
    </button>
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
  .field input.readonly {
    color: var(--fg-tertiary);
    font-size: 12.5px;
    font-family: var(--font-mono);
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
    margin: 4px 0 0;
    padding: 10px 12px;
    background: color-mix(in srgb, var(--danger) 14%, transparent);
    color: var(--danger);
    border-radius: 8px;
    font-size: 13px;
  }
  .danger {
    margin-top: 24px;
    height: 44px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--danger) 12%, transparent);
    color: var(--danger);
    font-weight: 600;
    font-size: 14px;
  }
  .danger:hover:not(:disabled) {
    background: color-mix(in srgb, var(--danger) 22%, transparent);
  }
</style>
