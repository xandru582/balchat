<script>
  import Avatar from './Avatar.svelte'
  import { fmtSidebarTime, previewText } from '../lib/format.js'

  let { contact, active = false, onSelect, onDelete, onEdit } = $props()

  let preview = $derived(previewText(contact))
  let timeLabel = $derived(fmtSidebarTime(contact.last_created_at))
  let unread = $derived(contact.unread_count || 0)
</script>

<div
  class="row"
  class:active
  role="button"
  tabindex="0"
  aria-pressed={active}
  onclick={() => onSelect?.(contact)}
  onkeydown={(e) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault()
      onSelect?.(contact)
    }
  }}
>
  <Avatar label={contact.label} seed={contact.onion_address} size={40} />
  <div class="meta">
    <div class="top">
      <span class="name">{contact.label}</span>
      {#if timeLabel}
        <time class="time">{timeLabel}</time>
      {/if}
    </div>
    <div class="bottom">
      <span class="preview">{preview || contact.onion_address}</span>
      {#if unread > 0 && !active}
        <span class="unread">{unread > 99 ? '99+' : unread}</span>
      {:else if contact.has_group}
        <span class="dot-active" title="Sesión activa"></span>
      {/if}
    </div>
  </div>

  <div class="actions">
    <button
      class="action-btn edit"
      type="button"
      onclick={(e) => { e.stopPropagation(); onEdit?.(contact, e) }}
      aria-label="Editar contacto {contact.label}"
      title="Editar contacto"
    >
      <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="M11 2l3 3-8 8H3v-3l8-8z"/>
      </svg>
    </button>
    <button
      class="action-btn del"
      type="button"
      onclick={(e) => { e.stopPropagation(); onDelete?.(contact, e) }}
      aria-label="Borrar contacto {contact.label}"
      title="Borrar contacto"
    >
      <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
        <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>
      </svg>
    </button>
  </div>
</div>

<style>
  .row {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 12px;
    margin: 0;
    border-radius: 8px;
    background: transparent;
    color: inherit;
    text-align: left;
    transition: background 100ms ease;
    position: relative;
  }
  .row:hover { background: var(--bg-hover); }
  .row.active {
    background: var(--bg-selected-strong);
    color: var(--fg-on-selected);
  }
  .row.active .preview,
  .row.active .time { color: rgba(255, 255, 255, 0.85); }
  .row.active .dot-active { background: rgba(255, 255, 255, 0.9); }

  .meta { min-width: 0; }
  .top {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }
  .name {
    font-weight: 600;
    font-size: 13.5px;
    color: inherit;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .time {
    font-size: 11px;
    color: var(--fg-tertiary);
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
  }
  .bottom {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    margin-top: 1px;
  }
  .preview {
    font-size: 12.5px;
    color: var(--fg-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
    flex: 1;
  }
  .unread {
    background: var(--accent);
    color: #fff;
    font-size: 10.5px;
    font-weight: 600;
    padding: 1px 7px;
    border-radius: 999px;
    line-height: 1.4;
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
  }
  .row.active .unread { background: rgba(255, 255, 255, 0.32); }
  .dot-active {
    width: 6px; height: 6px;
    border-radius: 50%;
    background: var(--success);
    flex-shrink: 0;
  }
  .actions {
    display: flex;
    gap: 2px;
    opacity: 0;
    transition: opacity 100ms ease;
  }
  .row:hover .actions,
  .actions:focus-within { opacity: 1; }
  .action-btn {
    width: 22px; height: 22px;
    border-radius: 50%;
    color: var(--fg-tertiary);
    display: flex; align-items: center; justify-content: center;
    transition: background 100ms ease, color 100ms ease;
  }
  .action-btn.edit:hover { background: var(--accent); color: #fff; }
  .action-btn.del:hover  { background: var(--danger); color: #fff; }
  .row.active .action-btn { color: rgba(255, 255, 255, 0.7); }
  .row.active .action-btn:hover { background: rgba(255, 255, 255, 0.22); color: #fff; }
</style>
