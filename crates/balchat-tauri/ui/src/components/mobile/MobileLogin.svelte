<script>
  let {
    mode = 'open',
    busy = false,
    error = '',
    onUnlock,
    onCreate,
    onSwitchMode,
  } = $props()

  let passphrase = $state('')
  let passphrase2 = $state('')
  let label = $state('')
  let touched = $state(false)

  let mismatch = $derived(touched && passphrase2.length > 0 && passphrase !== passphrase2)
  let tooShort = $derived(touched && passphrase.length > 0 && passphrase.length < 4)
</script>

<div class="screen">
  <div class="content">
    <div class="brand">
      <div class="logo">
        <svg viewBox="0 0 80 80" width="72" height="72">
          <defs>
            <linearGradient id="mlogin" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stop-color="#0a84ff"/>
              <stop offset="100%" stop-color="#5856d6"/>
            </linearGradient>
          </defs>
          <rect x="0" y="0" width="80" height="80" rx="20" fill="url(#mlogin)"/>
          <path d="M20 32c0-6 5-11 11-11h18c6 0 11 5 11 11v8c0 6-5 11-11 11h-13l-10 7v-7c-3-2-6-6-6-11v-8z"
                fill="#fff" opacity="0.95"/>
        </svg>
      </div>
      <h1>balchat</h1>
      <p class="tagline">Mensajes cifrados sobre Tor</p>
    </div>

    {#if mode === 'create'}
      <form onsubmit={(e) => { e.preventDefault(); touched = true; if (passphrase.length >= 4 && passphrase === passphrase2) onCreate?.(passphrase, label.trim()) }}>
        <label class="field">
          <span>Tu nombre <em>opcional</em></span>
          <input type="text" placeholder="Cómo quieres que te llamen" bind:value={label} disabled={busy} autocapitalize="words" />
        </label>
        <label class="field">
          <span>Contraseña</span>
          <input
            type="password"
            placeholder="Mínimo 4 caracteres"
            bind:value={passphrase}
            oninput={() => (touched = true)}
            disabled={busy}
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
          {#if mismatch}<small class="warn">No coinciden</small>{/if}
        </label>

        <button
          type="submit"
          class="primary"
          disabled={busy || !passphrase || passphrase.length < 4 || passphrase !== passphrase2}
        >
          {busy ? 'Creando cuenta…' : 'Crear mi cuenta'}
        </button>

        <button type="button" class="link" onclick={() => onSwitchMode?.('open')} disabled={busy}>
          Ya tengo una cuenta
        </button>
      </form>
    {:else}
      <form onsubmit={(e) => { e.preventDefault(); if (passphrase) onUnlock?.(passphrase) }}>
        <label class="field">
          <span>Contraseña</span>
          <input
            type="password"
            placeholder="Tu contraseña"
            bind:value={passphrase}
            disabled={busy}
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
      <p class="error">{error}</p>
    {/if}
  </div>

  <p class="footer">Sin servidores · sin teléfono · sin email</p>
</div>

<style>
  .screen {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: space-between;
    padding: calc(40px + var(--safe-top)) 22px calc(28px + var(--safe-bottom));
    background:
      radial-gradient(circle at 15% 0%, color-mix(in srgb, var(--accent) 22%, transparent), transparent 50%),
      radial-gradient(circle at 90% 100%, color-mix(in srgb, #5856d6 18%, transparent), transparent 50%),
      var(--bg);
  }
  .content {
    width: 100%;
    max-width: 380px;
    display: flex;
    flex-direction: column;
    gap: 22px;
  }
  .brand {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
    margin-bottom: 8px;
  }
  .logo { filter: drop-shadow(0 8px 24px color-mix(in srgb, var(--accent) 36%, transparent)); }
  h1 {
    margin: 0;
    font-size: 28px;
    font-weight: 700;
    letter-spacing: -0.02em;
  }
  .tagline {
    margin: -8px 0 0;
    font-size: 13.5px;
    color: var(--fg-secondary);
  }
  form { display: flex; flex-direction: column; gap: 14px; }
  .field { display: flex; flex-direction: column; gap: 6px; }
  .field > span {
    font-size: 12.5px;
    font-weight: 500;
    color: var(--fg-secondary);
    padding-left: 4px;
  }
  .field em {
    font-style: normal;
    color: var(--fg-tertiary);
    margin-left: 4px;
  }
  .field input {
    height: 46px;
    padding: 0 14px;
    border-radius: 12px;
    background: var(--bg-elevated);
  }
  .warn {
    color: var(--warning);
    font-size: 12px;
    padding-left: 4px;
  }
  .primary {
    margin-top: 4px;
    height: 48px;
    border-radius: 12px;
    background: var(--accent);
    color: #fff;
    font-weight: 600;
    font-size: 15px;
  }
  .primary:disabled { opacity: 0.5; }
  .primary:active:not(:disabled) { background: var(--accent-hover); }
  .link {
    margin-top: 2px;
    color: var(--accent);
    font-size: 13.5px;
    text-align: center;
    padding: 8px;
  }
  .error {
    margin: 4px 0 0;
    padding: 10px 12px;
    background: color-mix(in srgb, var(--danger) 14%, transparent);
    color: var(--danger);
    border-radius: 8px;
    font-size: 13px;
    text-align: center;
  }
  .footer {
    font-size: 11.5px;
    color: var(--fg-tertiary);
    letter-spacing: 0.02em;
  }
</style>
