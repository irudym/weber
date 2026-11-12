<script lang="ts">
    import type { Snippet } from 'svelte';
    import type { IconName } from './Icon.svelte';
    import Modal from './Modal.svelte';

    let {
      title,
      icon = 'trash',
      confirmLabel,
      danger = false,
      onconfirm,
      onclose,
      children
    } : {
      title: string,
      icon?: IconName,
      confirmLabel: string,
      danger?: boolean,
      onconfirm: () => Promise<string | null>
      onclose: () => void,
      children?: Snippet
    } = $props();

    let busy = $state(false);
    let error = $state('');

    async function confirm() {
      busy = true;
      error = '';
      const problem  = await onconfirm();
      busy = false;
      if (problem) error = problem; else onclose()
    }

</script>

<Modal {title} {icon} {onclose} size="compact" >
    <div class="dialog-content">
        {#if error}
            <p class="alert" role="alert">{error}</p>
        {/if}

        <div class="dialog-message">{@render children?.()}</div>

        <div class="dialog-actions">
            <button class="btn" type="button" onclick={onclose} disabled={busy}>Cancel</button>
            <button class="btn {danger ? 'btn-danger' : 'btn-primary'}" type="button" onclick={confirm} disabled={busy}>
                {busy ? "Working..." : confirmLabel}
            </button>
        </div>
    </div>

</Modal>
