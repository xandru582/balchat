<script>
  import { colorFor, initialsFor } from '../lib/avatar.js'

  let { label = '', seed = '', size = 36 } = $props()

  let color = $derived(colorFor(seed || label))
  let initials = $derived(initialsFor(label, seed))
  let fontSize = $derived(Math.round(size * 0.42))
</script>

<div
  class="avatar"
  style="--size: {size}px; --bg: {color}; --fs: {fontSize}px"
  aria-hidden="true"
>
  <span>{initials}</span>
</div>

<style>
  .avatar {
    width: var(--size);
    height: var(--size);
    border-radius: 50%;
    background: var(--bg);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: #fff;
    font-weight: 600;
    font-size: var(--fs);
    line-height: 1;
    flex-shrink: 0;
    letter-spacing: -0.01em;
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.08);
  }
</style>
