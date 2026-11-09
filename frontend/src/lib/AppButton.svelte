<script lang="ts">
  import Icon, { type IconName } from './Icon.svelte'
  let { icon, title, onclick }: { icon: IconName; title: string, onclick: () => void } = $props()
</script>


<button class="btn" onclick={onclick}>
    <Icon name={icon} />
    <span>{title}</span>
</button>
