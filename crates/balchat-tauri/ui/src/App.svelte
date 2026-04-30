<script>
  import { onMount, onDestroy, tick } from 'svelte'
  import { invoke } from '@tauri-apps/api/core'
  import { listen } from '@tauri-apps/api/event'
  import { open as openDialog } from '@tauri-apps/plugin-dialog'

  // -------- Estado --------
  let unlocked = $state(false)
  let passphrase = $state('')
  let passphrase2 = $state('')
  let label = $state('me')
  let unlockError = $state('')
  let busy = $state(false)
  let mode = $state('unknown') // 'unknown' | 'open' | 'create'

  let myId = $state({ onion: '', queue: '', relay: '' })
  let contacts = $state([])
  let selected = $state(null) // contacto seleccionado
  let log = $state([])        // mensajes (live + recibidos)
  let draft = $state('')

  let daemonStatus = $state('idle') // idle | starting | running | error

  // -- Agregar contacto (form plegable en el aside) --
  let showAddContact = $state(false)
  let newContact = $state({ label: '', onion: '', relay: '', queue: '', pubkey: '' })
  let addContactError = $state('')
  let addingContact = $state(false)

  // -- Auto-scroll: mantener el chat log pegado al final cuando llegan/se
  // envían mensajes. Se recibe el binding del div .chat-log y, en cada cambio
  // de `log`, después de que Svelte aplica el DOM (`tick()`), llevamos el
  // scrollTop al final. No vale la pena distinguir entre "el usuario está
  // mirando histórico" y "está al final" en esta primera versión: cualquier
  // mensaje nuevo siempre debe ser visible.
  let chatLogEl = $state(null)

  $effect(() => {
    // Tracking explícito de las dependencias: log + selected.
    void log
    void selected
    if (!chatLogEl) return
    tick().then(() => {
      if (chatLogEl) chatLogEl.scrollTop = chatLogEl.scrollHeight
    })
  })

  // -- Auto-lock por inactividad --
  // El usuario lo configura en Settings (default 5 min, 0 = desactivado). El timer
  // se resetea con cualquier interacción (mousemove/keydown/touchstart/click/scroll).
  // Cuando expira, llamamos lockVault() y el AppState olvida el vault → la UI vuelve
  // a la pantalla de unlock automáticamente.
  let autoLockMinutes = $state(5)
  let autoLockTimer = null

  // -- Settings panel (engranaje en header) --
  let showSettings = $state(false)
  let settingsRelay = $state('')
  let settingsAutoLock = $state(5)
  let settingsKpCount = $state(10)
  let settingsBusy = $state(false)
  let settingsError = $state('')
  let settingsInfo = $state('')

  let unlistenMessage
  let unlistenStatus

  // -------- Lifecycle --------
  onMount(async () => {
    unlistenMessage = await listen('balchat://message', (e) => {
      const m = e.payload
      // Los eventos del backend no traen `created_at`; lo seteamos con la hora
      // local para que se renderice con la misma marca HH:MM que los persistidos.
      if (m.created_at == null) m.created_at = Math.floor(Date.now() / 1000)
      // 'received' viene con `from` = onion del peer; los demás (info/error/sent)
      // los mostramos siempre. Filtramos los recibidos por contacto activo: si
      // llega un mensaje de otro peer, ya quedó persistido en el vault y se
      // verá al seleccionarlo — no contamina la conversación abierta.
      if (m.kind === 'received') {
        if (selected && m.from && m.from === selected.onion_address) {
          log = [...log, m]
        }
      } else {
        log = [...log, m]
      }
    })
    unlistenStatus = await listen('balchat://status', (e) => {
      daemonStatus = e.payload.status
    })
    // Decidir si mostrar "abrir" o "crear" según exista el vault.
    try {
      const exists = await invoke('vault_exists')
      mode = exists ? 'open' : 'create'
    } catch (e) {
      mode = 'open' // fallback al UI clásico
    }
  })

  onDestroy(() => {
    unlistenMessage?.()
    unlistenStatus?.()
    stopAutoLock()
  })

  function resetAutoLockTimer() {
    if (!unlocked) return
    if (autoLockTimer) clearTimeout(autoLockTimer)
    if (autoLockMinutes <= 0) return // 0 = desactivado
    autoLockTimer = setTimeout(() => { lockVault('inactivity') }, autoLockMinutes * 60 * 1000)
  }

  function stopAutoLock() {
    if (autoLockTimer) { clearTimeout(autoLockTimer); autoLockTimer = null }
    window.removeEventListener('mousemove', resetAutoLockTimer)
    window.removeEventListener('keydown', resetAutoLockTimer)
    window.removeEventListener('touchstart', resetAutoLockTimer)
    window.removeEventListener('click', resetAutoLockTimer)
    window.removeEventListener('scroll', resetAutoLockTimer, true)
  }

  function startAutoLock() {
    window.addEventListener('mousemove', resetAutoLockTimer)
    window.addEventListener('keydown', resetAutoLockTimer)
    window.addEventListener('touchstart', resetAutoLockTimer)
    window.addEventListener('click', resetAutoLockTimer)
    window.addEventListener('scroll', resetAutoLockTimer, true)
    resetAutoLockTimer()
  }

  /** Cierra el vault en el backend y vuelve la UI a la pantalla de unlock.
   *  `reason` es solo para el log y se ignora en el backend. */
  async function lockVault(reason = 'manual') {
    try {
      await invoke('lock_vault')
    } catch (e) {
      // Aun si falla, fingimos que está locked en el frontend para no quedar
      // colgados con UI desbloqueada y backend en estado raro.
      console.warn('lock_vault falló:', e)
    }
    stopAutoLock()
    unlocked = false
    selected = null
    log = []
    contacts = []
    myId = { onion: '', queue: '', relay: '' }
    daemonStatus = 'idle'
    if (reason === 'inactivity') {
      unlockError = 'sesión cerrada por inactividad'
    }
  }

  // -------- Acciones --------
  async function unlock() {
    busy = true
    unlockError = ''
    try {
      myId = await invoke('unlock_vault', { passphrase })
      contacts = await invoke('list_contacts')
      await loadSettings()
      unlocked = true
      passphrase = ''
      startAutoLock()
    } catch (e) {
      unlockError = String(e)
    } finally {
      busy = false
    }
  }

  async function createVault() {
    if (passphrase.length < 4) { unlockError = 'passphrase muy corta (mín 4)'; return }
    if (passphrase !== passphrase2) { unlockError = 'las passphrases no coinciden'; return }
    busy = true
    unlockError = ''
    try {
      myId = await invoke('create_vault', { passphrase, label })
      contacts = await invoke('list_contacts')
      await loadSettings()
      unlocked = true
      passphrase = ''
      passphrase2 = ''
      startAutoLock()
    } catch (e) {
      unlockError = String(e)
    } finally {
      busy = false
    }
  }

  async function loadSettings() {
    try {
      const s = await invoke('get_settings_cmd')
      autoLockMinutes = s.auto_lock_minutes
      settingsAutoLock = s.auto_lock_minutes
    } catch (e) {
      console.warn('get_settings_cmd:', e)
    }
  }

  function openSettings() {
    settingsError = ''
    settingsInfo = ''
    settingsRelay = myId.relay || ''
    settingsAutoLock = autoLockMinutes
    showSettings = true
  }

  async function saveRelay() {
    settingsBusy = true
    settingsError = ''
    settingsInfo = ''
    try {
      myId = await invoke('set_my_relay_cmd', { relayOnion: settingsRelay.trim() })
      settingsInfo = 'relay actualizado'
    } catch (e) {
      settingsError = String(e)
    } finally {
      settingsBusy = false
    }
  }

  async function saveAutoLock() {
    settingsBusy = true
    settingsError = ''
    settingsInfo = ''
    try {
      const v = Math.max(0, Math.min(1440, Number(settingsAutoLock) | 0))
      await invoke('set_settings_cmd', { autoLockMinutes: v })
      autoLockMinutes = v
      resetAutoLockTimer()
      settingsInfo = v === 0 ? 'auto-lock desactivado' : `auto-lock = ${v} min`
    } catch (e) {
      settingsError = String(e)
    } finally {
      settingsBusy = false
    }
  }

  async function publishKp() {
    settingsBusy = true
    settingsError = ''
    settingsInfo = ''
    try {
      const v = Math.max(1, Math.min(100, Number(settingsKpCount) | 0))
      const pool = await invoke('publish_kp_cmd', { count: v })
      settingsInfo = `${v} KeyPackages publicados; pool ahora = ${pool}`
    } catch (e) {
      settingsError = String(e)
    } finally {
      settingsBusy = false
    }
  }

  async function exportVault() {
    settingsBusy = true
    settingsError = ''
    settingsInfo = ''
    try {
      const dir = await openDialog({ directory: true, multiple: false })
      if (!dir) { settingsBusy = false; return }
      const dst = await invoke('export_vault_cmd', { targetDir: String(dir) })
      settingsInfo = `vault copiado: ${dst}`
    } catch (e) {
      settingsError = String(e)
    } finally {
      settingsBusy = false
    }
  }

  async function startDaemon() {
    daemonStatus = 'starting'
    try {
      await invoke('start_daemon')
    } catch (e) {
      daemonStatus = 'error'
      log = [...log, { kind: 'error', text: String(e) }]
    }
  }

  async function refreshContacts() {
    contacts = await invoke('list_contacts')
  }

  /** Borra el contacto y todo su histórico tras una confirmación.
   *  Si está seleccionado actualmente, deselecciona y vacía el log. */
  async function deleteContact(c, ev) {
    // Evitar que el click delegue al `<li>` y abra el chat después de borrar.
    ev?.stopPropagation()
    const ok = window.confirm(`Borrar contacto "${c.label}" y todo su historial?\nEsto no se puede deshacer.`)
    if (!ok) return
    try {
      await invoke('delete_contact_cmd', { peer: c.onion_address })
      if (selected?.onion_address === c.onion_address) {
        selected = null
        log = []
      }
      await refreshContacts()
    } catch (e) {
      log = [...log, { kind: 'error', text: `delete-contact: ${e}`, created_at: Math.floor(Date.now() / 1000) }]
    }
  }

  async function submitNewContact() {
    addContactError = ''
    if (!newContact.label.trim()) { addContactError = 'label requerido'; return }
    if (!newContact.onion.trim()) { addContactError = 'onion requerido'; return }
    addingContact = true
    try {
      await invoke('add_contact_cmd', {
        label: newContact.label.trim(),
        onion: newContact.onion.trim(),
        relay: newContact.relay.trim() || null,
        queueHex: newContact.queue.trim() || null,
        pubkeyHex: newContact.pubkey.trim() || null,
      })
      newContact = { label: '', onion: '', relay: '', queue: '', pubkey: '' }
      showAddContact = false
      await refreshContacts()
    } catch (e) {
      addContactError = String(e)
    } finally {
      addingContact = false
    }
  }

  async function send() {
    if (!selected || !draft.trim()) return
    const text = draft
    draft = ''
    const now = Math.floor(Date.now() / 1000)
    try {
      await invoke('send_text', { peer: selected.onion_address, text })
      log = [...log, { kind: 'sent', to: selected.label, text, created_at: now }]
    } catch (e) {
      log = [...log, { kind: 'error', text: `send: ${e}`, created_at: now }]
    }
  }

  async function attachFile() {
    if (!selected || !selected.has_group) return
    let path
    try {
      path = await openDialog({ multiple: false, directory: false })
    } catch (e) {
      log = [...log, { kind: 'error', text: `dialog: ${e}`, created_at: Math.floor(Date.now() / 1000) }]
      return
    }
    if (!path) return // cancelado
    const filename = String(path).split(/[\\/]/).pop()
    const now = Math.floor(Date.now() / 1000)
    try {
      await invoke('send_file_path', { peer: selected.onion_address, path: String(path) })
      log = [...log, { kind: 'sent', to: selected.label, text: `[archivo: ${filename}]`, created_at: now }]
    } catch (e) {
      log = [...log, { kind: 'error', text: `send-file: ${e}`, created_at: now }]
    }
  }

  async function selectContact(c) {
    selected = c
    // Cargar histórico desde el vault y mapearlo al formato del log live.
    // `direction == 'sent'`  → kind:'sent', to:label
    // `direction == 'received'` → kind:'received', from:onion, from_label, text
    // Los archivos se renderizan como un mensaje de texto con prefijo.
    try {
      const history = await invoke('list_messages_cmd', { peer: c.onion_address, limit: 200 })
      log = history.map((m) => {
        const text = m.kind === 'file' ? `[archivo: ${m.body}]` : m.body
        if (m.direction === 'sent') {
          return { kind: 'sent', to: c.label, text, created_at: m.created_at }
        }
        return { kind: 'received', from: c.onion_address, from_label: c.label, text, created_at: m.created_at }
      })
    } catch (e) {
      log = [{ kind: 'error', text: `cargar histórico: ${e}`, created_at: Math.floor(Date.now() / 1000) }]
    }
    // Marcar como leído y refrescar el sidebar para que el badge "no leído" se vaya.
    try {
      await invoke('mark_contact_read_cmd', { peer: c.onion_address })
      await refreshContacts()
    } catch (e) {
      console.warn('mark_contact_read_cmd:', e)
    }
  }

  /** Resumen corto del último mensaje para el sidebar. Recorta a 60 chars. */
  function previewText(c) {
    if (!c.last_body) return ''
    const prefix = c.last_direction === 'sent' ? '→ ' : ''
    const body = c.last_kind === 'file' ? `📎 ${c.last_body}` : c.last_body
    const flat = body.replace(/\s+/g, ' ').trim()
    const max = 60
    return prefix + (flat.length > max ? flat.slice(0, max - 1) + '…' : flat)
  }

  /** Formatea Unix epoch (seg) como "HH:MM" en la zona local del cliente. */
  function fmtTime(ts) {
    if (ts == null) return ''
    const d = new Date(ts * 1000)
    const hh = String(d.getHours()).padStart(2, '0')
    const mm = String(d.getMinutes()).padStart(2, '0')
    return `${hh}:${mm}`
  }

  function fmtMsg(m) {
    const t = fmtTime(m.created_at)
    const prefix = t ? `[${t}] ` : ''
    if (m.kind === 'received') return `${prefix}← ${m.from_label || m.from} : ${m.text}`
    if (m.kind === 'sent')     return `${prefix}→ ${m.to} : ${m.text}`
    if (m.kind === 'error')    return `${prefix}! ${m.text}`
    if (m.kind === 'info')     return `${prefix}· ${m.text}`
    return JSON.stringify(m)
  }

  // -- Copiar el onion propio al clipboard (sutil feedback de 1.5 s) --
  let copyState = $state('idle') // 'idle' | 'ok' | 'err'
  async function copyMyOnion() {
    const v = (myId.onion || '').replace(/:\d+$/, '') // sin :1234
    if (!v) return
    try {
      await navigator.clipboard.writeText(v)
      copyState = 'ok'
    } catch (e) {
      copyState = 'err'
    }
    setTimeout(() => (copyState = 'idle'), 1500)
  }
</script>

<main>
  {#if !unlocked}
    <div class="login">
      <h1>balchat</h1>
      <p>Chat 1:1 cifrado E2E sobre Tor</p>

      {#if mode === 'create'}
        <p class="hint">No hay vault todavía. Creá uno nuevo:</p>
        <input
          type="text"
          placeholder="Tu nombre / label (opcional)"
          bind:value={label}
          disabled={busy}
        />
        <input
          type="password"
          placeholder="Passphrase (mín 4 chars)"
          bind:value={passphrase}
          disabled={busy}
        />
        <input
          type="password"
          placeholder="Repetí la passphrase"
          bind:value={passphrase2}
          onkeydown={(e) => e.key === 'Enter' && createVault()}
          disabled={busy}
        />
        <button onclick={createVault} disabled={busy || !passphrase}>
          {busy ? 'Creando...' : 'Crear vault'}
        </button>
        <p class="hint">
          <a href={null} onclick={() => (mode = 'open')}>¿ya tenés uno? abrir existente</a>
        </p>
      {:else}
        <input
          type="password"
          placeholder="Passphrase"
          bind:value={passphrase}
          onkeydown={(e) => e.key === 'Enter' && unlock()}
          disabled={busy}
        />
        <button onclick={unlock} disabled={busy || !passphrase}>
          {busy ? 'Abriendo...' : 'Abrir vault'}
        </button>
        <p class="hint">
          <a href={null} onclick={() => (mode = 'create')}>¿primera vez? crear vault nuevo</a>
        </p>
      {/if}

      {#if unlockError}
        <p class="error">{unlockError}</p>
      {/if}
    </div>
  {:else}
    <header>
      <div>
        <strong>{myId.onion || '(corre `balchat host` en CLI)'}</strong>
        {#if myId.onion}
          <button
            class="copy-btn"
            onclick={copyMyOnion}
            title="Copiar mi onion al portapapeles"
            aria-label="Copiar mi onion"
          >
            {copyState === 'ok' ? '✓' : copyState === 'err' ? '✗' : '⎘'}
          </button>
        {/if}
        · queue: <code>{(myId.queue || '').slice(0, 12)}…</code>
        · relay: {myId.relay || '(none)'}
      </div>
      <div>
        <span class="status status-{daemonStatus}">{daemonStatus}</span>
        {#if daemonStatus === 'idle' || daemonStatus === 'error'}
          <button onclick={startDaemon}>Arrancar daemon</button>
        {/if}
        <button onclick={refreshContacts} title="Refrescar contactos">↻</button>
        <button class="lock-btn" onclick={openSettings} title="Settings (relay, KeyPackages, auto-lock, export)">
          ⚙
        </button>
        <button class="lock-btn" onclick={() => lockVault('manual')} title="Cerrar sesión (lock)">
          🔒
        </button>
      </div>
    </header>

    <div class="layout">
      <aside>
        <div class="aside-header">
          <h3>Contactos</h3>
          <button
            class="add-toggle"
            onclick={() => { showAddContact = !showAddContact; addContactError = '' }}
            title={showAddContact ? 'Cancelar' : 'Agregar contacto'}
          >
            {showAddContact ? '×' : '+'}
          </button>
        </div>

        {#if showAddContact}
          <div class="add-form">
            <input
              type="text"
              placeholder="Label (ej: alice)"
              bind:value={newContact.label}
              disabled={addingContact}
            />
            <input
              type="text"
              placeholder="onion (xxx.onion[:1234])"
              bind:value={newContact.onion}
              disabled={addingContact}
            />
            <input
              type="text"
              placeholder="relay onion (opcional)"
              bind:value={newContact.relay}
              disabled={addingContact}
            />
            <input
              type="text"
              placeholder="queue id hex 64 chars (opcional)"
              bind:value={newContact.queue}
              disabled={addingContact}
            />
            <input
              type="text"
              placeholder="pubkey hex (opcional, cross-sign)"
              bind:value={newContact.pubkey}
              disabled={addingContact}
            />
            <button onclick={submitNewContact} disabled={addingContact}>
              {addingContact ? 'Guardando…' : 'Guardar contacto'}
            </button>
            {#if addContactError}
              <p class="error">{addContactError}</p>
            {/if}
          </div>
        {/if}

        {#if contacts.length === 0}
          <p class="muted">(sin contactos — tocá <strong>+</strong> para agregar)</p>
        {:else}
          <ul role="listbox">
            {#each contacts as c}
              <li
                role="option"
                tabindex="0"
                aria-selected={selected?.onion_address === c.onion_address}
                class:active={selected?.onion_address === c.onion_address}
                onclick={() => selectContact(c)}
                onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && selectContact(c)}
              >
                <div class="contact-row">
                  <div class="contact-info">
                    <div class="contact-top">
                      <strong>{c.label}</strong>
                      {#if c.last_created_at}
                        <small class="ts">{fmtTime(c.last_created_at)}</small>
                      {/if}
                    </div>
                    {#if c.last_body}
                      <div class="preview">{previewText(c)}</div>
                    {:else}
                      <small>{c.onion_address}</small>
                    {/if}
                    <div class="contact-tags">
                      {#if c.has_group}
                        <span class="badge">activo</span>
                      {/if}
                      {#if c.unread_count > 0 && selected?.onion_address !== c.onion_address}
                        <span class="badge unread">{c.unread_count}</span>
                      {/if}
                    </div>
                  </div>
                  <button
                    class="del-contact"
                    onclick={(ev) => deleteContact(c, ev)}
                    title="Borrar contacto y su historial"
                    aria-label="Borrar contacto {c.label}"
                  >×</button>
                </div>
              </li>
            {/each}
          </ul>
        {/if}
      </aside>

      <section>
        {#if selected}
          <div class="chat-header">
            chat con <strong>{selected.label}</strong>
            <small>{selected.onion_address}</small>
          </div>
          <div class="chat-log" bind:this={chatLogEl}>
            {#each log as m}
              <div class="msg msg-{m.kind}">{fmtMsg(m)}</div>
            {/each}
          </div>
          <div class="chat-input">
            <button class="attach" onclick={attachFile} disabled={!selected.has_group} title="Adjuntar archivo">
              Archivo
            </button>
            <input
              type="text"
              placeholder={selected.has_group ? 'Mensaje...' : 'Necesitás handshake live primero (CLI: balchat connect)'}
              bind:value={draft}
              onkeydown={(e) => e.key === 'Enter' && send()}
              disabled={!selected.has_group}
            />
            <button onclick={send} disabled={!draft.trim() || !selected.has_group}>
              Enviar
            </button>
          </div>
        {:else}
          <div class="empty">selecciona un contacto</div>
        {/if}
      </section>
    </div>

    {#if showSettings}
      <div
        class="modal-backdrop"
        onclick={() => (showSettings = false)}
        onkeydown={(e) => e.key === 'Escape' && (showSettings = false)}
        role="presentation"
      >
        <div
          class="modal"
          onclick={(e) => e.stopPropagation()}
          onkeydown={(e) => e.stopPropagation()}
          role="dialog"
          aria-label="Settings"
          tabindex="-1"
        >
          <div class="modal-header">
            <h3>Settings</h3>
            <button class="lock-btn" onclick={() => (showSettings = false)} aria-label="Cerrar">×</button>
          </div>

          <section class="settings-section">
            <h4>Mi relay</h4>
            <p class="hint">Onion del relay donde recibís blobs offline. Compartilo con quien quiera contactarte.</p>
            <div class="row">
              <input
                type="text"
                placeholder="xxxxxx.onion[:1235]"
                bind:value={settingsRelay}
                disabled={settingsBusy}
              />
              <button onclick={saveRelay} disabled={settingsBusy || !settingsRelay.trim()}>Guardar</button>
            </div>
          </section>

          <section class="settings-section">
            <h4>KeyPackages en mi relay</h4>
            <p class="hint">Para que peers offline puedan invitarte a grupos. Necesita el daemon corriendo.</p>
            <div class="row">
              <input
                type="number"
                min="1"
                max="100"
                bind:value={settingsKpCount}
                disabled={settingsBusy}
                style="width: 80px"
              />
              <button onclick={publishKp} disabled={settingsBusy || daemonStatus !== 'running'}>Publicar</button>
            </div>
          </section>

          <section class="settings-section">
            <h4>Auto-lock</h4>
            <p class="hint">Cierra el vault tras N minutos sin actividad. <code>0</code> desactiva.</p>
            <div class="row">
              <input
                type="number"
                min="0"
                max="1440"
                bind:value={settingsAutoLock}
                disabled={settingsBusy}
                style="width: 80px"
              />
              <span>min</span>
              <button onclick={saveAutoLock} disabled={settingsBusy}>Guardar</button>
            </div>
          </section>

          <section class="settings-section">
            <h4>Backup del vault</h4>
            <p class="hint">Copia <code>vault.db</code> + <code>.salt</code> a un directorio. Mantiene la passphrase original.</p>
            <button onclick={exportVault} disabled={settingsBusy}>Elegir destino…</button>
          </section>

          {#if settingsError}<p class="error">{settingsError}</p>{/if}
          {#if settingsInfo}<p class="info">{settingsInfo}</p>{/if}
        </div>
      </div>
    {/if}
  {/if}
</main>

<style>
  :global(body) {
    margin: 0;
    font-family: -apple-system, system-ui, sans-serif;
    background: #1e1e2e;
    color: #cdd6f4;
  }
  main {
    height: 100vh;
    display: flex;
    flex-direction: column;
  }
  .login {
    margin: auto;
    width: 320px;
    text-align: center;
  }
  .login h1 { color: #89b4fa; margin-bottom: 0.25rem; }
  .login p  { color: #a6adc8; margin-top: 0; }
  .login input, .login button {
    width: 100%;
    padding: 0.5rem 0.75rem;
    margin: 0.25rem 0;
    border-radius: 4px;
    border: 1px solid #45475a;
    background: #313244;
    color: #cdd6f4;
    font-size: 1rem;
  }
  .login button { background: #89b4fa; color: #1e1e2e; cursor: pointer; }
  .login button:disabled { opacity: 0.5; cursor: not-allowed; }
  .error { color: #f38ba8; }
  .hint  { color: #6c7086; font-size: 0.85rem; margin-top: 0.5rem; }
  .hint a { color: #89b4fa; cursor: pointer; text-decoration: underline; }

  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.5rem 1rem;
    background: #181825;
    border-bottom: 1px solid #313244;
    font-size: 0.85rem;
  }
  header code { background: #313244; padding: 0 4px; border-radius: 2px; }
  header button { margin-left: 0.5rem; cursor: pointer; }
  .copy-btn {
    background: #313244;
    color: #cdd6f4;
    border: 0;
    border-radius: 3px;
    padding: 0 6px;
    font-size: 0.85rem;
    margin-left: 0.25rem;
    cursor: pointer;
  }
  .copy-btn:hover { background: #45475a; }
  .lock-btn {
    background: #45475a;
    color: #cdd6f4;
    border: 0;
    border-radius: 3px;
    padding: 2px 8px;
    font-size: 0.85rem;
    cursor: pointer;
  }
  .lock-btn:hover { background: #585b70; }
  .status { padding: 2px 8px; border-radius: 12px; font-size: 0.75rem; }
  .status-idle    { background: #45475a; }
  .status-starting{ background: #f9e2af; color: #1e1e2e; }
  .status-running { background: #a6e3a1; color: #1e1e2e; }
  .status-error   { background: #f38ba8; color: #1e1e2e; }

  .layout { display: flex; flex: 1; overflow: hidden; }
  aside {
    width: 260px;
    background: #11111b;
    border-right: 1px solid #313244;
    overflow-y: auto;
    padding: 0.5rem;
  }
  aside h3 { margin: 0.25rem 0.5rem; color: #89b4fa; }
  aside ul { list-style: none; padding: 0; margin: 0; }
  .aside-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0 0.25rem;
  }
  .add-toggle {
    background: #313244;
    color: #cdd6f4;
    border: 0;
    border-radius: 4px;
    padding: 2px 10px;
    font-size: 1rem;
    cursor: pointer;
    line-height: 1.2;
  }
  .add-toggle:hover { background: #45475a; }
  .add-form {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    padding: 0.5rem;
    margin: 0.25rem 0;
    background: #181825;
    border: 1px solid #313244;
    border-radius: 4px;
  }
  .add-form input {
    padding: 0.4rem 0.5rem;
    border: 1px solid #45475a;
    background: #313244;
    color: #cdd6f4;
    border-radius: 3px;
    font-size: 0.85rem;
  }
  .add-form button {
    margin-top: 0.25rem;
    padding: 0.4rem;
    background: #89b4fa;
    color: #1e1e2e;
    border: 0;
    border-radius: 3px;
    cursor: pointer;
    font-size: 0.9rem;
  }
  .add-form button:disabled { opacity: 0.5; cursor: not-allowed; }
  .add-form .error { font-size: 0.8rem; margin: 0.25rem 0 0; }
  aside li {
    padding: 0.5rem;
    margin: 0.25rem 0;
    border-radius: 4px;
    cursor: pointer;
    display: flex; flex-direction: column;
  }
  aside li:hover { background: #181825; }
  aside li.active { background: #313244; }
  aside small { color: #6c7086; font-size: 0.7rem; word-break: break-all; }
  .badge { background: #a6e3a1; color: #1e1e2e; font-size: 0.65rem; padding: 0 4px; border-radius: 8px; margin-top: 2px; margin-right: 0.25rem; }
  .badge.unread { background: #89b4fa; color: #1e1e2e; padding: 0 6px; font-weight: 600; }
  .preview { color: #a6adc8; font-size: 0.78rem; margin-top: 2px; word-break: break-word; overflow: hidden; text-overflow: ellipsis; max-height: 2.4em; line-height: 1.2em; }
  .contact-top { display: flex; justify-content: space-between; align-items: baseline; gap: 0.5rem; }
  .contact-top .ts { color: #6c7086; font-size: 0.7rem; flex-shrink: 0; }
  .contact-tags { display: flex; flex-wrap: wrap; align-items: center; }
  .muted { color: #6c7086; }

  .contact-row {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 0.25rem;
    width: 100%;
  }
  .contact-info { display: flex; flex-direction: column; min-width: 0; flex: 1; }
  .contact-info small { word-break: break-all; }
  .del-contact {
    background: transparent;
    color: #6c7086;
    border: 0;
    padding: 0 6px;
    font-size: 1.1rem;
    cursor: pointer;
    border-radius: 3px;
    flex-shrink: 0;
    align-self: flex-start;
  }
  .del-contact:hover { background: #f38ba8; color: #1e1e2e; }

  section {
    flex: 1;
    display: flex;
    flex-direction: column;
  }
  .chat-header { padding: 0.5rem 1rem; border-bottom: 1px solid #313244; }
  .chat-log { flex: 1; padding: 1rem; overflow-y: auto; font-family: ui-monospace, monospace; font-size: 0.9rem; }
  .msg-sent     { color: #89b4fa; }
  .msg-received { color: #a6e3a1; }
  .msg-error    { color: #f38ba8; }
  .msg-info     { color: #a6adc8; }
  .chat-input {
    display: flex;
    padding: 0.5rem;
    border-top: 1px solid #313244;
    gap: 0.5rem;
  }
  .chat-input input { flex: 1; padding: 0.5rem; border: 1px solid #45475a; border-radius: 4px; background: #313244; color: #cdd6f4; }
  .chat-input button { padding: 0.5rem 1rem; background: #89b4fa; color: #1e1e2e; border: 0; border-radius: 4px; cursor: pointer; }
  .chat-input button:disabled { opacity: 0.5; cursor: not-allowed; }
  .chat-input button.attach { background: #45475a; color: #cdd6f4; }

  .empty {
    margin: auto;
    color: #6c7086;
  }

  /* -- Settings modal -- */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .modal {
    background: #1e1e2e;
    border: 1px solid #45475a;
    border-radius: 8px;
    padding: 1rem 1.25rem;
    width: min(420px, 90vw);
    max-height: 90vh;
    overflow-y: auto;
  }
  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 0.5rem;
  }
  .modal-header h3 { margin: 0; color: #89b4fa; }
  .settings-section { margin-bottom: 1rem; }
  .settings-section h4 { margin: 0.25rem 0; color: #cdd6f4; font-size: 0.95rem; }
  .settings-section .hint { color: #6c7086; font-size: 0.78rem; margin: 0.25rem 0 0.4rem; }
  .settings-section .row {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    flex-wrap: wrap;
  }
  .settings-section input[type=text],
  .settings-section input[type=number] {
    flex: 1;
    padding: 0.4rem 0.5rem;
    border: 1px solid #45475a;
    background: #313244;
    color: #cdd6f4;
    border-radius: 3px;
    font-size: 0.85rem;
    min-width: 0;
  }
  .settings-section button {
    padding: 0.4rem 0.75rem;
    background: #89b4fa;
    color: #1e1e2e;
    border: 0;
    border-radius: 3px;
    cursor: pointer;
    font-size: 0.85rem;
  }
  .settings-section button:disabled { opacity: 0.5; cursor: not-allowed; }
  .info { color: #a6e3a1; font-size: 0.85rem; margin: 0.5rem 0 0; }
</style>
