<script lang="ts">
  import Icon from './Icon.svelte';
  import AppButton from './AppButton.svelte';

  let { user, onadd }: { user: string, onadd: () => void } = $props()

  const now = () =>
    new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false })

  let clock = $state(now())

  // Runs once when the taskbar appears; the returned function cleans up.
  $effect(() => {
    const timer = setInterval(() => (clock = now()), 10_000)
    return () => clearInterval(timer)
  })
</script>

<!-- Always at the bottom, clear of the iPhone home indicator -->
<footer class="taskbar">
    <!-- Add button -->
    <AppButton icon="add" title="Add Process" onclick={onadd}/>
  <div class="tray">
    <span class="tray-user">{user}</span>
    <time>{clock}</time>
  </div>
  <!-- A real form POST: logout is handled by weber, not by the Svelte app -->
  <form method="post" action="/logout">
    <button class="btn" type="submit" aria-label="Log out">
      <Icon name="logout" /><span class="hide-sm">Log out</span>
    </button>
  </form>
</footer>
