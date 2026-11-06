<script lang="ts">
  import Backdrop from './lib/Backdrop.svelte'
  import LogViewer from './lib/LogViewer.svelte'
  import ProcessCard from './lib/ProcessCard.svelte'
  import Taskbar from './lib/Taskbar.svelte'
  import { api, type ProcStatus } from './lib/api'

  let procs = $state<ProcStatus[]>([])
  let loaded = $state(false)
  let user = $state('')
  // Which process's logs are open, and the button that opened them (focus returns there)
  let logs = $state<{ name: string; opener: HTMLElement } | null>(null)

  const running = $derived(procs.filter((p) => p.running).length)

  async function loadStatus() {
    try {
      procs = await (await api('/api/status')).json()
      loaded = true
    } catch {
      // network hiccup: keep the last known status, retry in 2 s
    }
  }

  function closeLogs() {
    const opener = logs?.opener
    logs = null
    opener?.focus()
  }

  $effect(() => {
    api('/api/me')
      .then((r) => r.json())
      .then((me: { user: string }) => (user = me.user))
      .catch(() => {})

    loadStatus()
    const timer = setInterval(loadStatus, 2000)
    return () => clearInterval(timer)
  })
</script>

<Backdrop />

<main class="dash" id="top">
  <header class="dash-header">
    <h1 class="wordmark">weber</h1>
    <p class="tagline">
      {loaded ? `${running} of ${procs.length} processes running` : 'Checking processes…'}
    </p>
  </header>

  <section class="proc-list" aria-label="Processes">
    {#each procs as proc (proc.name)}
      <ProcessCard {proc} onchange={loadStatus} onlogs={(opener) => (logs = { name: proc.name, opener })} />
    {/each}
  </section>
</main>

<Taskbar {user} />

{#if logs}
  <LogViewer name={logs.name} onclose={closeLogs} />
{/if}
