/* Deterministic avatar helpers — same onion always renders the same color
   and the same one or two-letter monogram. Hash is FNV-1a (32 bit), enough
   for ~12-color palette spread. */

const PALETTE = [
  '#ff3b30', '#ff9500', '#ffcc00', '#34c759',
  '#00c7be', '#30b0c7', '#007aff', '#5856d6',
  '#af52de', '#ff2d55', '#a2845e', '#5ac8fa',
]

function fnv1a(str) {
  let h = 0x811c9dc5
  for (let i = 0; i < str.length; i++) {
    h ^= str.charCodeAt(i)
    h = (h + ((h << 1) + (h << 4) + (h << 7) + (h << 8) + (h << 24))) >>> 0
  }
  return h
}

/** Stable color picked from PALETTE by hashing `key` (e.g. an onion address). */
export function colorFor(key) {
  if (!key) return PALETTE[0]
  return PALETTE[fnv1a(String(key)) % PALETTE.length]
}

/** Up to 2 chars from a label. Falls back to the first char of `fallbackKey`. */
export function initialsFor(label, fallbackKey) {
  const src = (label || '').trim()
  if (src) {
    const parts = src.split(/\s+/).filter(Boolean)
    if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase()
    return (parts[0][0] + parts[1][0]).toUpperCase()
  }
  return ((fallbackKey || '?')[0] || '?').toUpperCase()
}
