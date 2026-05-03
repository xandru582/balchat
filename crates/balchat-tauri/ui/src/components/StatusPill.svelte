<script>
  let { status = 'idle' } = $props()

  const LABELS = {
    idle: 'desconectado',
    starting: 'conectando',
    running: 'conectado',
    error: 'sin conexión',
  }
  let label = $derived(LABELS[status] ?? status)
</script>

<span class="pill" data-status={status} title="Estado de la red: {label}">
  <span class="dot"></span>
  <span class="text">{label}</span>
</span>

<style>
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 10px 3px 8px;
    border-radius: 999px;
    background: var(--bg-pill);
    font-size: 11.5px;
    color: var(--fg-secondary);
    font-weight: 500;
    line-height: 1;
    user-select: none;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--status-idle);
    box-shadow: 0 0 0 0 currentColor;
  }
  .pill[data-status="idle"]     .dot { background: var(--status-idle); }
  .pill[data-status="starting"] .dot { background: var(--status-starting); animation: pulse 1.4s infinite ease-in-out; }
  .pill[data-status="running"]  .dot { background: var(--status-running); box-shadow: 0 0 0 3px color-mix(in srgb, var(--status-running) 24%, transparent); }
  .pill[data-status="error"]    .dot { background: var(--status-error); }

  @keyframes pulse {
    0%, 100% { opacity: 1; transform: scale(1); }
    50%      { opacity: 0.5; transform: scale(0.85); }
  }
</style>
