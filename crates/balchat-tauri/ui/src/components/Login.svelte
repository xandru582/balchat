<script>
  let {
    mode = 'open',         // 'open' | 'create'
    busy = false,
    error = '',
    onUnlock,              // (passphrase: string) => void
    onCreate,              // (passphrase, label) => void
    onSwitchMode,          // (newMode) => void
  } = $props()

  let passphrase = $state('')
  let passphrase2 = $state('')
  let label = $state('')
  let touched = $state(false)

  let mismatch = $derived(touched && passphrase2.length > 0 && passphrase !== passphrase2)
  let tooShort = $derived(touched && passphrase.length > 0 && passphrase.length < 4)

  function submitOpen() {
    if (!passphrase) return
    onUnlock?.(passphrase)
  }
  function submitCreate() {
    touched = true
    if (passphrase.length < 4) return
    if (passphrase !== passphrase2) return
    onCreate?.(passphrase, label.trim())
  }
</script>

<div class="screen">
  <!-- Macros title bar is hidden, so this strip provides the only drag handle. -->
  <div class="dragstrip" data-tauri-drag-region></div>
  <div class="card">
    <div class="brand">
      <div class="logo" aria-hidden="true">
        <svg viewBox="0 0 56 56" width="56" height="56">
          <defs>
            <linearGradient id="bgrad" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stop-color="#0a84ff"/>
              <stop offset="100%" stop-color="#5856d6"/>
            </linearGradient>
          </defs>
          <rect x="0" y="0" width="56" height="56" rx="14" fill="url(#bgrad)"/>
          <path d="M14 22c0-4.4 3.6-8 8-8h12c4.4 0 8 3.6 8 8v6c0 4.4-3.6 8-8 8h-9l-7 5v-5c-2.4-1.4-4-4-4-7v-7z"
                fill="#fff" opacity="0.95"/>
        </svg>
      </div>
      <h1>balchat</h1>
      <p class="tagline">Mensajes cifrados sobre la red Tor</p>
    </div>

    {#if mode === 'create'}
      <form onsubmit={(e) => { e.preventDefault(); submitCreate() }}>
        <label class="field">
          <span>Tu nombre <em>opcional</em></span>
          <input
            type="text"
            placeholder="Cómo quieres que te llamen"
            bind:value={label}
            disabled={busy}
            autocomplete="off"
          />
        </label>

        <label class="field">
          <span>Contraseña</span>
          <input
            type="password"
            placeholder="Mínimo 4 caracteres"
            bind:value={passphrase}
            oninput={() => (touched = true)}
            disabled={busy}
            autofocus
          />
          {#if tooShort}<small class="warn">Demasiado corta (mín 4)</small>{/if}
        </label>

        <label class="field">
          <span>Confirmar contraseña</span>
          <input
            type="password"
            placeholder="Repite tu contraseña"
            bind:value={passphrase2}
            oninput={() => (touched = true)}
            disabled={busy}
          />
          {#if mismatch}<small class="warn">Las contraseñas no coinciden</small>{/if}
        </label>

        <button
          type="submit"
          class="primary"
          disabled={busy || !passphrase || passphrase.length < 4 || passphrase !== passphrase2}
        >
          {busy ? 'Creando cuenta…' : 'Crear mi cuenta'}
        </button>

        <button type="button" class="link" onclick={() => onSwitchMode?.('open')} disabled={busy}>
          Ya tengo una cuenta — abrirla
        </button>
      </form>
    {:else}
      <form onsubmit={(e) => { e.preventDefault(); submitOpen() }}>
        <label class="field">
          <span>Contraseña</span>
          <input
            type="password"
            placeholder="Tu contraseña"
            bind:value={passphrase}
            disabled={busy}
            autofocus
          />
        </label>

        <button type="submit" class="primary" disabled={busy || !passphrase}>
          {busy ? 'Abriendo…' : 'Entrar'}
        </button>

        <button type="button" class="link" onclick={() => onSwitchMode?.('create')} disabled={busy}>
          Primera vez — crear cuenta
        </button>
      </form>
    {/if}

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
  </div>

  <p class="footer">
    Sin servidores · sin teléfono · sin email
  </p>
</div>

<style>
  .screen {
    height: 100vh;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 24px;
    position: relative;
    background:
      radial-gradient(circle at 20% 0%, color-mix(in srgb, var(--accent) 18%, transparent), transparent 40%),
      radial-gradient(circle at 80% 100%, color-mix(in srgb, #5856d6 16%, transparent), transparent 40%),
      var(--bg);
  }
  .dragstrip {
    position: absolute;
    top: 0; left: 0; right: 0;
    height: var(--titlebar-h);
    z-index: 1;
  }
  .card {
    width: 360px;
    max-width: 100%;
    padding: 28px 28px 22px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
  }
  .brand {
    display: flex;
    flex-direction: column;
    align-items: center;
    margin-bottom: 22px;
  }
  .logo {
    margin-bottom: 12px;
    filter: drop-shadow(0 4px 14px color-mix(in srgb, var(--accent) 38%, transparent));
  }
  h1 {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
    letter-spacing: -0.02em;
  }
  .tagline {
    margin: 4px 0 0;
    font-size: 12.5px;
    color: var(--fg-secondary);
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .field > span {
    font-size: 12px;
    font-weight: 500;
    color: var(--fg-secondary);
  }
  .field em {
    font-style: normal;
    font-weight: 400;
    color: var(--fg-tertiary);
    margin-left: 4px;
  }
  .field input { padding: 8px 11px; font-size: 13.5px; }
  .warn { color: var(--warning); font-size: 11.5px; }

  .primary {
    margin-top: 4px;
    padding: 9px 14px;
    background: var(--accent);
    color: #fff;
    border-radius: 8px;
    font-weight: 600;
    font-size: 13.5px;
    transition: background 100ms ease, transform 80ms ease;
  }
  .primary:hover:not(:disabled) { background: var(--accent-hover); }
  .primary:active:not(:disabled) { transform: translateY(1px); }

  .link {
    margin-top: 2px;
    color: var(--accent);
    font-size: 12.5px;
    text-align: center;
    padding: 4px;
  }
  .link:hover:not(:disabled) { text-decoration: underline; }

  .error {
    margin: 14px 0 0;
    padding: 8px 10px;
    background: color-mix(in srgb, var(--danger) 14%, transparent);
    color: var(--danger);
    border-radius: 6px;
    font-size: 12.5px;
  }

  .footer {
    margin-top: 18px;
    font-size: 11px;
    color: var(--fg-tertiary);
    letter-spacing: 0.02em;
  }
</style>
