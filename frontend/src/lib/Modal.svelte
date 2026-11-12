<script lang="ts">
    import Icon, { type IconName } from './Icon.svelte'
    import type { Snippet } from 'svelte'
    let { title, icon, onclose, overlayclick=false, children, size = 'full' }: { title: string; icon: IconName, onclose: () => void, overlayclick?: boolean, children?: Snippet, size?: 'full' | 'compact' } = $props()

    const titleId = $props.id();

    function onOverlayClick(e: MouseEvent) {
      if (overlayclick && e.target === e.currentTarget) onclose()
    }

    let closeBtn = $state<HTMLButtonElement>()

    // On open: move keyboard focus into the dialog and stop the page behind it from scrolling.
    // On close (component removed): undo the scroll lock.
    $effect(() => {
      closeBtn?.focus()
      document.body.style.overflow = 'hidden'
      return () => {
        document.body.style.overflow = ''
      }
    })
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onclose()} />

<!-- Clicking the dimmed area closes the viewer; keyboard users have Escape and the close button -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="overlay {size}" onclick={onOverlayClick}>
  <div class="window {size === 'full' ? 'log-window' : 'dialog-window'}" role="dialog" aria-modal="true" aria-labelledby={titleId}>
    <div class="titlebar">
      <Icon name={icon} />
      <h2 class="titlebar-text" id={titleId}>{title}</h2>
      <button class="btn btn-icon" type="button" aria-label="Close {title}" bind:this={closeBtn} onclick={onclose}>
        <Icon name="close" />
      </button>
    </div>
    {@render children?.()}
  </div>
</div>
