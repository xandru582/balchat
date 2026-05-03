<script>
  import { onMount, onDestroy, tick } from 'svelte'
  import { invoke } from '@tauri-apps/api/core'
  import { listen } from '@tauri-apps/api/event'
  import { open as openDialog } from '@tauri-apps/plugin-dialog'

  import Login from './components/Login.svelte'
  import Sidebar from './components/Sidebar.svelte'
  import ChatView from './components/ChatView.svelte'
  import Settings from './components/Settings.svelte'
  import MobileLogin from './components/mobile/MobileLogin.svelte'
  import MobileShell from './components/mobile/MobileShell.svelte'
  import { isMobile } from './lib/platform.js'

  let mobile = $state(false)
  $effect(() => {
    const unsub = isMobile.subscribe((v) => {
      mobile = v
      if (v) document.documentElement.classList.add('mobile')
      else document.documentElement.classList.remove('mobile')
    })
    return unsub
  })

  // -- Vault / session state --
  let unlocked = $state(false)
  let mode = $state('unknown') // 'unknown' | 'open' | 'create'
  let unlockBusy = $state(false)
  let unlockError = $state('')

  // -- Identity & contacts --
  let myId = $state({ onion: '', queue: '', relay: '' })
  let contacts = $state([])
  let selected = $state(null)
  let log = $state([])

  // -- Daemon status --
  let daemonStatus = $state('idle')

  // -- Settings panel --
  let showSettings = $state(false)
  let autoLockMinutes = $state(5)

  // -- Handshake state (per session, transient) --
  let handshakeBusy = $state(false)

  let unlistenMessage
  let unlistenStatus
  let unlistenContactUpdated
  let autoLockTimer = null

  /** Refresh the contact list and re-bind `selected` to its fresh row so reactive
   *  props (like `has_group`) propagate. */
  async function refreshContactsAndSelected() {
    contacts = await invoke('list_contacts')
    if (selected) {
      const fresh = contacts.find((c) => c.onion_address === selected.onion_address)
      if (fresh) selected = fresh
    }
  }

  // -------- Lifecycle --------
  onMount(async () => {
    unlistenMessage = await listen('balchat://message', (e) => {
      const m = e.payload
      if (m.created_at == null) m.created_at = Math.floor(Date.now() / 1000)
      // 'received' from a different peer is already persisted; only inject into
      // the live log if it matches the open conversation.
      if (m.kind === 'received') {
        if (selected && m.from && m.from === selected.onion_address) {
          log = [...log, m]
        }
        // Refresh sidebar previews & unread counts regardless.
        refreshContacts()
      } else {
        log = [...log, m]
      }
    })
    unlistenContactUpdated = await listen('balchat://contact-updated', async () => {
      // Triggered when a handshake (initiator or responder) completed for any
      // contact. Refresh so `has_group` flips and the chat banner disappears.
      try {
        await refreshContactsAndSelected()
      } catch (e) {
        console.warn('refresh on contact-updated:', e)
      }
    })
    unlistenStatus = await listen('balchat://status', async (e) => {
      const prev = daemonStatus
      daemonStatus = e.payload.status
      // Once the daemon is up, the onion service may have just been provisioned;
      // re-read MyId so the header shows the address with a copy button.
      if (e.payload.status === 'running' && prev !== 'running' && unlocked) {
        try {
          myId = await invoke('get_my_id')
        } catch (err) {
          console.warn('get_my_id:', err)
        }
      }
    })

    try {
      const exists = await invoke('vault_exists')
      mode = exists ? 'open' : 'create'
    } catch {
      mode = 'open'
    }
  })

  onDestroy(() => {
    unlistenMessage?.()
    unlistenStatus?.()
    unlistenContactUpdated?.()
    stopAutoLock()
  })

  // -------- Auto-lock --------
  function resetAutoLockTimer() {
    if (!unlocked) return
    if (autoLockTimer) clearTimeout(autoLockTimer)
    if (autoLockMinutes <= 0) return
    autoLockTimer = setTimeout(() => lockVault('inactivity'), autoLockMinutes * 60 * 1000)
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

  // -------- Vault actions --------
  async function unlock(passphrase) {
    unlockBusy = true; unlockError = ''
    try {
      myId = await invoke('unlock_vault', { passphrase })
      contacts = await invoke('list_contacts')
      await loadSettings()
      unlocked = true
      startAutoLock()
    } catch (e) {
      unlockError = String(e)
    } finally {
      unlockBusy = false
    }
  }

  async function createVault(passphrase, label) {
    unlockBusy = true; unlockError = ''
    try {
      myId = await invoke('create_vault', { passphrase, label })
      contacts = await invoke('list_contacts')
      await loadSettings()
      unlocked = true
      startAutoLock()
    } catch (e) {
      unlockError = String(e)
    } finally {
      unlockBusy = false
    }
  }

  async function lockVault(reason = 'manual') {
    try { await invoke('lock_vault') } catch (e) { console.warn('lock_vault:', e) }
    stopAutoLock()
    unlocked = false
    selected = null
    log = []
    contacts = []
    myId = { onion: '', queue: '', relay: '' }
    daemonStatus = 'idle'
    showSettings = false
    if (reason === 'inactivity') unlockError = 'Tu sesión se cerró por inactividad'
  }

  // -------- Settings --------
  async function loadSettings() {
    try {
      const s = await invoke('get_settings_cmd')
      autoLockMinutes = s.auto_lock_minutes
    } catch (e) {
      console.warn('get_settings_cmd:', e)
    }
  }
  async function saveRelay(relay) {
    myId = await invoke('set_my_relay_cmd', { relayOnion: relay })
  }
  async function saveAutoLock(minutes) {
    await invoke('set_settings_cmd', { autoLockMinutes: minutes })
    autoLockMinutes = minutes
    resetAutoLockTimer()
  }
  async function publishKp(count) {
    return await invoke('publish_kp_cmd', { count })
  }
  async function exportVault() {
    const dir = await openDialog({ directory: true, multiple: false })
    if (!dir) return null
    return await invoke('export_vault_cmd', { targetDir: String(dir) })
  }

  // -------- Daemon --------
  async function startDaemon() {
    daemonStatus = 'starting'
    try {
      await invoke('start_daemon')
    } catch (e) {
      daemonStatus = 'error'
      log = [...log, { kind: 'error', text: String(e), created_at: Math.floor(Date.now() / 1000) }]
    }
  }

  // -------- Contacts --------
  async function refreshContacts() {
    contacts = await invoke('list_contacts')
  }

  async function addContact({ label, onion, relay, queueHex, pubkeyHex }) {
    await invoke('add_contact_cmd', { label, onion, relay, queueHex, pubkeyHex })
    await refreshContacts()
  }

  async function deleteContact(c) {
    const ok = window.confirm(`¿Borrar contacto «${c.label}» y todo su historial?\nEsta acción no se puede deshacer.`)
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

  async function selectContact(c) {
    selected = c
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
    try {
      await invoke('mark_contact_read_cmd', { peer: c.onion_address })
      await refreshContacts()
    } catch (e) {
      console.warn('mark_contact_read_cmd:', e)
    }
  }

  // -------- Messages --------
  async function sendMessage(text) {
    if (!selected) return
    const now = Math.floor(Date.now() / 1000)
    try {
      await invoke('send_text', { peer: selected.onion_address, text })
      log = [...log, { kind: 'sent', to: selected.label, text, created_at: now }]
    } catch (e) {
      log = [...log, { kind: 'error', text: `send: ${e}`, created_at: now }]
    }
  }

  async function startHandshake(peer) {
    if (handshakeBusy) return
    handshakeBusy = true
    try {
      await invoke('connect_cmd', { peer })
      // Backend also emits `balchat://contact-updated` which refreshes; this is
      // a belt-and-braces in case the listener is delayed.
      await refreshContactsAndSelected()
    } catch (e) {
      log = [...log, { kind: 'error', text: `handshake: ${e}`, created_at: Math.floor(Date.now() / 1000) }]
    } finally {
      handshakeBusy = false
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
    if (!path) return
    const filename = String(path).split(/[\\/]/).pop()
    const now = Math.floor(Date.now() / 1000)
    try {
      await invoke('send_file_path', { peer: selected.onion_address, path: String(path) })
      log = [...log, { kind: 'sent', to: selected.label, text: `[archivo: ${filename}]`, created_at: now }]
    } catch (e) {
      log = [...log, { kind: 'error', text: `send-file: ${e}`, created_at: now }]
    }
  }

  // -------- Clipboard --------
  async function copyOnion() {
    const v = (myId.onion || '').replace(/:\d+$/, '')
    if (!v) return 'err'
    try {
      await navigator.clipboard.writeText(v)
      return 'ok'
    } catch {
      return 'err'
    }
  }
</script>

<main>
  {#if !unlocked}
    {#if mode !== 'unknown'}
      {#if mobile}
        <MobileLogin
          {mode}
          busy={unlockBusy}
          error={unlockError}
          onUnlock={unlock}
          onCreate={createVault}
          onSwitchMode={(m) => { mode = m; unlockError = '' }}
        />
      {:else}
        <Login
          {mode}
          busy={unlockBusy}
          error={unlockError}
          onUnlock={unlock}
          onCreate={createVault}
          onSwitchMode={(m) => { mode = m; unlockError = '' }}
        />
      {/if}
    {/if}
  {:else if mobile}
    <MobileShell
      {contacts}
      {selected}
      {log}
      {daemonStatus}
      {myId}
      {autoLockMinutes}
      {handshakeBusy}
      onSelect={selectContact}
      onAddContact={addContact}
      onSend={sendMessage}
      onAttach={attachFile}
      onStartDaemon={startDaemon}
      onLock={() => lockVault('manual')}
      onCopyOnion={copyOnion}
      onHandshake={startHandshake}
      onSaveRelay={saveRelay}
      onSaveAutoLock={saveAutoLock}
      onPublishKp={publishKp}
      onExportVault={exportVault}
    />
  {:else}
    <div class="app">
      <Sidebar
        {contacts}
        {selected}
        onSelect={selectContact}
        onDelete={deleteContact}
        onAddContact={addContact}
      />
      <ChatView
        {selected}
        {log}
        {daemonStatus}
        {myId}
        {handshakeBusy}
        onSend={sendMessage}
        onAttach={attachFile}
        onStartDaemon={startDaemon}
        onRefresh={refreshContacts}
        onLock={() => lockVault('manual')}
        onSettings={() => (showSettings = true)}
        onCopyOnion={copyOnion}
        onHandshake={startHandshake}
      />
    </div>

    {#if showSettings}
      <Settings
        initialRelay={myId.relay || ''}
        initialAutoLock={autoLockMinutes}
        {daemonStatus}
        onSaveRelay={saveRelay}
        onSaveAutoLock={saveAutoLock}
        onPublishKp={publishKp}
        onExportVault={exportVault}
        onClose={() => (showSettings = false)}
      />
    {/if}
  {/if}
</main>

<style>
  main {
    height: 100vh;
    width: 100vw;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .app {
    display: flex;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
</style>
