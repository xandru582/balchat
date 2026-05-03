<script>
  import { fmtTime } from '../lib/format.js'

  let { msg, showTail = true } = $props()

  let side = $derived(
    msg.kind === 'sent' ? 'sent' :
    msg.kind === 'received' ? 'recv' :
    'system'
  )

  /** Detect "[archivo: filename]" so we can render a file pill instead of raw text. */
  let fileName = $derived.by(() => {
    const m = /^\[archivo:\s*(.+)\]$/.exec(msg.text || '')
    return m ? m[1] : null
  })
</script>

{#if side === 'system'}
  <div class="system" data-kind={msg.kind}>
    <span>{msg.text}</span>
    {#if msg.created_at}<time>{fmtTime(msg.created_at)}</time>{/if}
  </div>
{:else}
  <div class="row" data-side={side}>
    <div class="bubble" class:no-tail={!showTail} data-side={side}>
      {#if fileName}
        <div class="file">
          <div class="file-icon" aria-hidden="true">📎</div>
          <div class="file-meta">
            <div class="file-name">{fileName}</div>
            <div class="file-sub">archivo recibido</div>
          </div>
        </div>
      {:else}
        <div class="text">{msg.text}</div>
      {/if}
      <time class="ts">{fmtTime(msg.created_at)}</time>
    </div>
  </div>
{/if}

<style>
  .row {
    display: flex;
    width: 100%;
    margin: 1px 0;
  }
  .row[data-side="sent"] { justify-content: flex-end; }
  .row[data-side="recv"] { justify-content: flex-start; }

  .bubble {
    max-width: 72%;
    padding: 7px 12px 7px 12px;
    border-radius: var(--radius-bubble);
    font-size: 14px;
    line-height: 1.36;
    word-wrap: break-word;
    overflow-wrap: anywhere;
    position: relative;
    user-select: text;
    -webkit-user-select: text;
    box-shadow: var(--shadow-sm);
  }
  .bubble[data-side="sent"] {
    background: var(--bg-bubble-sent);
    color: var(--fg-bubble-sent);
    border-bottom-right-radius: 6px;
  }
  .bubble[data-side="recv"] {
    background: var(--bg-bubble-recv);
    color: var(--fg-bubble-recv);
    border-bottom-left-radius: 6px;
  }
  .bubble.no-tail[data-side="sent"] { border-bottom-right-radius: var(--radius-bubble); }
  .bubble.no-tail[data-side="recv"] { border-bottom-left-radius: var(--radius-bubble); }

  .text { white-space: pre-wrap; }

  .ts {
    display: block;
    font-size: 10.5px;
    margin-top: 2px;
    opacity: 0.65;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .bubble[data-side="recv"] .ts { text-align: left; }

  .file {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 2px 0;
    min-width: 180px;
  }
  .file-icon {
    width: 36px; height: 36px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.18);
    display: flex; align-items: center; justify-content: center;
    font-size: 18px;
    flex-shrink: 0;
  }
  .bubble[data-side="recv"] .file-icon { background: rgba(0, 0, 0, 0.10); }
  .file-name { font-weight: 600; font-size: 13.5px; }
  .file-sub  { font-size: 11.5px; opacity: 0.75; }

  .system {
    display: flex;
    align-items: center;
    gap: 6px;
    justify-content: center;
    margin: 8px auto;
    padding: 4px 12px;
    border-radius: 999px;
    font-size: 11.5px;
    color: var(--fg-tertiary);
    background: var(--bg-pill);
    max-width: 80%;
    user-select: text;
    -webkit-user-select: text;
  }
  .system[data-kind="error"] { color: var(--danger); }
  .system time { opacity: 0.7; font-variant-numeric: tabular-nums; }
</style>
