<script module lang="ts">
  // The pixel icons, imported as plain SVG text at build time.
  import close from './icons/close.svg?raw'
  import logo from './icons/logo.svg?raw'
  import logout from './icons/logout.svg?raw'
  import logs from './icons/logs.svg?raw'
  import play from './icons/play.svg?raw'
  import stop from './icons/stop.svg?raw'
  import add from './icons/add.svg?raw'

  const icons = { close, logo, logout, logs, play, stop, add }
  export type IconName = keyof typeof icons
</script>

<script lang="ts">
    let { name }: { name: keyof typeof icons } = $props()
</script>

<!-- Safe: the markup comes from our own files, never from user input -->
{@html icons[name]}
