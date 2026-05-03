<script>
  /* Stack-style router for the mobile layout. Each screen owns the full viewport
     and slides in/out from the right (push) or left (pop) via Svelte transitions.
     The shell is purely presentational: it receives state + callbacks from
     App.svelte exactly like the desktop Sidebar/ChatView pair does. */
  import { fly } from 'svelte/transition'
  import { quintOut } from 'svelte/easing'

  import MobileHome from './MobileHome.svelte'
  import MobileChat from './MobileChat.svelte'
  import MobileNewContact from './MobileNewContact.svelte'
  import MobileSettings from './MobileSettings.svelte'

  let {
    contacts = [],
    selected = null,
    log = [],
    daemonStatus = 'idle',
    myId,
    autoLockMinutes = 5,
    handshakeBusy = false,
    onSelect,
    onAddContact,
    onSend,
    onAttach,
    onStartDaemon,
    onLock,
    onCopyOnion,
    onHandshake,
    onSaveRelay,
    onSaveAutoLock,
    onPublishKp,
    onExportVault,
  } = $props()

  // 'home' | 'chat' | 'new' | 'settings'
  let screen = $state('home')

  function openChat(c) {
    onSelect?.(c)
    screen = 'chat'
  }
  function backToHome() {
    screen = 'home'
  }
  function openNewContact() {
    screen = 'new'
  }
  function openSettings() {
    screen = 'settings'
  }

  async function saveContact(payload) {
    await onAddContact?.(payload)
  }

  // Slide direction: push goes left-to-right out of the new screen, pop goes the other way.
  // We just always animate the foreground screen sliding in from x=100% (push) and rely on
  // unmount to drop the previous one — Svelte handles the absolute-positioned transition.
</script>

<div class="shell">
  {#if screen === 'home'}
    <div class="page" in:fly={{ x: -40, duration: 200, easing: quintOut }}>
      <MobileHome
        {contacts}
        {daemonStatus}
        {myId}
        onOpenChat={openChat}
        onAddContact={openNewContact}
        onSettings={openSettings}
        {onLock}
        {onCopyOnion}
        {onStartDaemon}
      />
    </div>
  {:else if screen === 'chat'}
    <div class="page" in:fly={{ x: 80, duration: 220, easing: quintOut }}>
      <MobileChat
        contact={selected}
        {log}
        {daemonStatus}
        {handshakeBusy}
        onBack={backToHome}
        {onSend}
        {onAttach}
        {onHandshake}
      />
    </div>
  {:else if screen === 'new'}
    <div class="page" in:fly={{ x: 80, duration: 220, easing: quintOut }}>
      <MobileNewContact
        onBack={backToHome}
        onSave={saveContact}
      />
    </div>
  {:else if screen === 'settings'}
    <div class="page" in:fly={{ x: 80, duration: 220, easing: quintOut }}>
      <MobileSettings
        initialRelay={myId?.relay || ''}
        initialAutoLock={autoLockMinutes}
        {daemonStatus}
        {myId}
        onBack={backToHome}
        {onSaveRelay}
        {onSaveAutoLock}
        {onPublishKp}
        {onExportVault}
      />
    </div>
  {/if}
</div>

<style>
  .shell {
    position: relative;
    height: 100vh;
    width: 100vw;
    overflow: hidden;
    background: var(--bg);
  }
  .page {
    position: absolute;
    inset: 0;
    background: var(--bg);
  }
</style>
