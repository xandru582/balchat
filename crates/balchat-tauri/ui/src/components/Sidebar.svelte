<script>
  import ContactRow from './ContactRow.svelte'

  let {
    contacts = [],
    selected = null,
    onSelect,
    onDelete,
    onAddContact,
    onUpdateContact,
  } = $props()

  let query = $state('')
  let showAdd = $state(false)
  let showAdvanced = $state(false)
  let addBusy = $state(false)
  let addError = $state('')
  let form = $state({ label: '', onion: '', relay: '', queue: '', pubkey: '' })

  // Edit mode: when set, the form switches to "edit existing contact" — onion is
  // read-only (it identifies the contact), label/relay/queue/pubkey are editable.
  let editing = $state(null) // contact being edited, or null
  function openEdit(c) {
    editing = c
    showAdd = false
    addError = ''
    form = {
      label: c.label || '',
      onion: c.onion_address || '',
      relay: c.relay_onion || '',
      queue: '', // hex shown only if user wants to overwrite (not pre-filled to avoid confusion)
      pubkey: '',
    }
    showAdvanced = !!(c.relay_onion || c.unread_count === undefined)
  }
  function cancelEdit() {
    editing = null
    addError = ''
    form = { label: '', onion: '', relay: '', queue: '', pubkey: '' }
  }

  let filtered = $derived.by(() => {
    const q = query.trim().toLowerCase()
    if (!q) return contacts
    return contacts.filter((c) =>
      (c.label || '').toLowerCase().includes(q) ||
      (c.onion_address || '').toLowerCase().includes(q)
    )
  })

  async function submit() {
    addError = ''
    if (!form.label.trim()) { addError = 'El nombre es obligatorio'; return }
    if (!editing && !form.onion.trim()) { addError = 'El código de chat es obligatorio'; return }
    addBusy = true
    try {
      if (editing) {
        await onUpdateContact?.({
          peer: editing.onion_address,
          label: form.label.trim(),
          relay: form.relay.trim(),       // empty string clears
          queueHex: form.queue.trim(),    // empty string clears
          pubkeyHex: form.pubkey.trim(),  // empty string clears
        })
        cancelEdit()
      } else {
        await onAddContact?.({
          label: form.label.trim(),
          onion: form.onion.trim(),
          relay: form.relay.trim() || null,
          queueHex: form.queue.trim() || null,
          pubkeyHex: form.pubkey.trim() || null,
        })
        form = { label: '', onion: '', relay: '', queue: '', pubkey: '' }
        showAdd = false
      }
    } catch (e) {
      addError = String(e)
    } finally {
      addBusy = false
    }
  }
</script>

<aside class="sidebar">
  <div class="titlebar" data-tauri-drag-region>
    <!-- Reserve space for macOS traffic lights on the left. The button stays no-drag. -->
    <div class="spacer" data-tauri-drag-region></div>
    <button
      class="icon-btn no-drag"
      type="button"
      onclick={() => { showAdd = !showAdd; addError = '' }}
      title={showAdd ? 'Cerrar' : 'Nuevo contacto'}
      aria-label={showAdd ? 'Cerrar' : 'Nuevo contacto'}
    >
      {#if showAdd}
        <svg viewBox="0 0 16 16" width="14" height="14"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>
      {:else}
        <svg viewBox="0 0 16 16" width="14" height="14"><path d="M8 3v10M3 8h10" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>
      {/if}
    </button>
  </div>

  <div class="search">
    <svg class="search-icon" viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
      <circle cx="7" cy="7" r="4.5" fill="none" stroke="currentColor" stroke-width="1.4"/>
      <path d="M10.5 10.5L13 13" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>
    </svg>
    <input
      type="text"
      placeholder="Buscar"
      bind:value={query}
    />
    {#if query}
      <button class="clear" onclick={() => (query = '')} title="Limpiar" aria-label="Limpiar búsqueda">
        <svg viewBox="0 0 16 16" width="11" height="11"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>
      </button>
    {/if}
  </div>

  {#if showAdd || editing}
    <form class="add-form" onsubmit={(e) => { e.preventDefault(); submit() }}>
      <div class="form-head">
        <h4>{editing ? `Editar ${editing.label}` : 'Nuevo contacto'}</h4>
        {#if editing}
          <button type="button" class="close-edit" onclick={cancelEdit} title="Cancelar">×</button>
        {/if}
      </div>
      <input type="text" placeholder="Nombre (ej: Alice)" bind:value={form.label} disabled={addBusy} />
      {#if !editing}
        <input type="text" placeholder="Código de chat de tu contacto" bind:value={form.onion} disabled={addBusy} />
      {:else}
        <input type="text" value={form.onion} disabled readonly class="readonly" title="El código de chat no se puede cambiar" />
      {/if}
      <button
        type="button"
        class="adv-toggle"
        onclick={() => (showAdvanced = !showAdvanced)}
      >{showAdvanced ? '− Ocultar opciones avanzadas' : '+ Opciones avanzadas'}</button>
      {#if showAdvanced}
        <input type="text" placeholder={editing ? 'Buzón offline (vacío = quitar)' : 'Buzón offline propio (opcional)'} bind:value={form.relay} disabled={addBusy} />
        <input type="text" placeholder="Queue ID 64-hex (opcional)" bind:value={form.queue} disabled={addBusy} />
        <input type="text" placeholder="Pubkey hex (opcional)" bind:value={form.pubkey} disabled={addBusy} />
      {/if}
      <button class="primary" type="submit" disabled={addBusy}>
        {addBusy ? 'Guardando…' : (editing ? 'Guardar cambios' : 'Guardar contacto')}
      </button>
      {#if addError}<p class="error">{addError}</p>{/if}
    </form>
  {/if}

  <div class="list">
    {#if filtered.length === 0}
      {#if contacts.length === 0}
        <div class="empty">
          <p>Sin contactos todavía</p>
          <small>Toca <strong>+</strong> arriba para añadir uno</small>
        </div>
      {:else}
        <div class="empty">
          <p>Sin resultados</p>
          <small>Nada que coincida con «{query}»</small>
        </div>
      {/if}
    {:else}
      {#each filtered as c (c.onion_address)}
        <ContactRow
          contact={c}
          active={selected?.onion_address === c.onion_address}
          onSelect={(x) => onSelect?.(x)}
          onDelete={(x, ev) => onDelete?.(x, ev)}
          onEdit={(x) => openEdit(x)}
        />
      {/each}
    {/if}
  </div>
</aside>

<style>
  .sidebar {
    width: var(--sidebar-w);
    flex-shrink: 0;
    height: 100vh;
    background: var(--bg-sidebar);
    backdrop-filter: saturate(180%) blur(28px);
    -webkit-backdrop-filter: saturate(180%) blur(28px);
    border-right: 1px solid var(--separator);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  /* Fallback when backdrop-filter is not available. */
  @supports not ((backdrop-filter: blur(1px)) or (-webkit-backdrop-filter: blur(1px))) {
    .sidebar { background: var(--bg-sidebar-solid); }
  }

  .titlebar {
    height: var(--titlebar-h);
    display: flex;
    align-items: center;
    padding: 0 8px 0 78px; /* 78px = ~space for macOS traffic lights */
    flex-shrink: 0;
  }
  .spacer { flex: 1; }
  .icon-btn {
    width: 26px; height: 26px;
    display: flex; align-items: center; justify-content: center;
    border-radius: 6px;
    color: var(--fg-secondary);
    transition: background 100ms ease, color 100ms ease;
  }
  .icon-btn:hover { background: var(--bg-hover); color: var(--fg); }

  .search {
    position: relative;
    margin: 4px 12px 8px;
    display: flex;
    align-items: center;
  }
  .search-icon {
    position: absolute;
    left: 9px;
    color: var(--fg-tertiary);
    pointer-events: none;
  }
  .search input {
    width: 100%;
    padding: 6px 28px 6px 28px;
    font-size: 13px;
    border-radius: 7px;
    background: var(--bg-pill);
    border: 1px solid transparent;
  }
  .search input:focus {
    background: var(--bg-input);
    border-color: var(--accent);
  }
  .clear {
    position: absolute;
    right: 6px;
    width: 18px; height: 18px;
    border-radius: 50%;
    color: var(--fg);
    background: var(--bg-pill);
    display: flex; align-items: center; justify-content: center;
  }
  .clear:hover { background: var(--border-strong); }

  .add-form {
    margin: 4px 12px 8px;
    padding: 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    display: flex;
    flex-direction: column;
    gap: 6px;
    box-shadow: var(--shadow-sm);
  }
  .add-form h4 {
    margin: 0;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--fg-secondary);
    letter-spacing: 0.01em;
    text-transform: uppercase;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .form-head {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 2px;
  }
  .close-edit {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    color: var(--fg-tertiary);
    background: transparent;
    font-size: 16px;
    line-height: 1;
    flex-shrink: 0;
  }
  .close-edit:hover { background: var(--bg-hover); color: var(--fg); }
  .add-form input.readonly {
    color: var(--fg-tertiary);
    cursor: not-allowed;
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .add-form input { font-size: 12.5px; padding: 6px 8px; }
  .adv-toggle {
    margin: 2px 0 0;
    background: transparent;
    color: var(--accent);
    font-size: 11.5px;
    text-align: left;
    padding: 4px 0;
  }
  .add-form .primary {
    margin-top: 4px;
    padding: 7px;
    background: var(--accent);
    color: #fff;
    border-radius: 6px;
    font-weight: 600;
    font-size: 12.5px;
  }
  .add-form .primary:hover:not(:disabled) { background: var(--accent-hover); }
  .add-form .error {
    margin: 4px 0 0;
    color: var(--danger);
    font-size: 11.5px;
  }

  .list {
    flex: 1;
    overflow-y: auto;
    padding: 4px 8px 12px;
  }
  .empty {
    padding: 28px 16px;
    text-align: center;
    color: var(--fg-tertiary);
  }
  .empty p { margin: 0 0 4px; font-size: 13px; color: var(--fg-secondary); }
  .empty small { font-size: 11.5px; }
  .empty strong { color: var(--accent); font-weight: 600; }
</style>
