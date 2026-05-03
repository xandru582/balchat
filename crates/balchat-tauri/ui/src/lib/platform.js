/* Reactive mobile-vs-desktop detection. Triggers on viewport resize so a
   desktop user that drags the window narrow also gets the mobile layout. */

import { readable } from 'svelte/store'

const QUERY = '(max-width: 720px), (pointer: coarse)'

export const isMobile = readable(matches(), (set) => {
  if (typeof window === 'undefined' || !window.matchMedia) return () => {}
  const mq = window.matchMedia(QUERY)
  const handler = () => set(mq.matches)
  mq.addEventListener('change', handler)
  // Also react to plain resizes (some webviews don't fire mq change reliably).
  window.addEventListener('resize', handler)
  return () => {
    mq.removeEventListener('change', handler)
    window.removeEventListener('resize', handler)
  }
})

function matches() {
  if (typeof window === 'undefined' || !window.matchMedia) return false
  return window.matchMedia(QUERY).matches
}
