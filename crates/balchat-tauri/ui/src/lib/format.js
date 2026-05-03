/* Time / preview formatters. All times are Unix epoch seconds (matching the
   backend's payloads). Locale comes from the OS — Intl picks it up via
   `undefined`. */

export function fmtTime(ts) {
  if (ts == null) return ''
  const d = new Date(ts * 1000)
  return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })
}

/** Sidebar-style relative timestamp: "14:32", "ayer", "lun", "12 mar". */
export function fmtSidebarTime(ts) {
  if (ts == null) return ''
  const d = new Date(ts * 1000)
  const now = new Date()
  const sameDay = d.toDateString() === now.toDateString()
  if (sameDay) return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })
  const diffMs = now - d
  const oneDay = 24 * 60 * 60 * 1000
  if (diffMs < 2 * oneDay) return 'ayer'
  if (diffMs < 7 * oneDay) return d.toLocaleDateString(undefined, { weekday: 'short' })
  return d.toLocaleDateString(undefined, { day: 'numeric', month: 'short' })
}

/** Day-divider label inside the chat log: "Hoy", "Ayer", or full date. */
export function fmtDayDivider(ts) {
  if (ts == null) return ''
  const d = new Date(ts * 1000)
  const now = new Date()
  const sameDay = d.toDateString() === now.toDateString()
  if (sameDay) return 'Hoy'
  const yesterday = new Date(now)
  yesterday.setDate(now.getDate() - 1)
  if (d.toDateString() === yesterday.toDateString()) return 'Ayer'
  return d.toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long' })
}

/** Compact preview shown in the contact list. Includes a "→" for outgoing. */
export function previewText(c) {
  if (!c.last_body) return ''
  const prefix = c.last_direction === 'sent' ? 'Tú: ' : ''
  const body = c.last_kind === 'file' ? `📎 ${c.last_body}` : c.last_body
  const flat = body.replace(/\s+/g, ' ').trim()
  const max = 64
  return prefix + (flat.length > max ? flat.slice(0, max - 1) + '…' : flat)
}

/** Truncate a long onion to "abc…xyz.onion" form for chat headers. */
export function shortOnion(onion) {
  if (!onion) return ''
  const bare = onion.replace(/:\d+$/, '')
  if (bare.length <= 22) return onion
  return bare.slice(0, 6) + '…' + bare.slice(-12)
}
