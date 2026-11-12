<script lang="ts">
    import Icon, { type IconName } from './Icon.svelte'

    let {title, label, icon, onclick }: {title: string, label: string, icon: IconName, onclick: () => void} = $props();
</script>

<button class="btn btn-icon caption-btn"
        type="button"
        aria-label="{title} {label}"
        title={title} onclick={onclick}>
    <Icon name={icon} />
</button>
