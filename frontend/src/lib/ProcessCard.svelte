<script lang="ts">
    import ConfirmDialog from './ConfirmDialog.svelte';
    import Icon from './Icon.svelte';
    import ProcessForm from './ProcessForm.svelte';
    import ToolButton from './ToolButton.svelte';
    import { api, formatBytes, type ProcStatus, joinArgs } from './api';

  let {
    proc,
    onchange,
    onlogs,
  }: {
    proc: ProcStatus
    onchange: () => void                  // ask the parent to reload the status
    onlogs: (opener: HTMLElement) => void // open the log viewer
  } = $props()

  let busy = $state(false)
  let note = $state('')
  let noteTimer: ReturnType<typeof setTimeout> | undefined
  let dialog = $state<'edit' | 'delete' | null>(null)

  const command = $derived([proc.config.program, joinArgs(proc.config.args)].filter(Boolean).join(' '))

  // $derived values recompute automatically whenever `proc` changes
  const failed = $derived(!proc.running && !!proc.last_exit && proc.last_exit !== 'exit code 0')

  const status = $derived(
    proc.running
      ? { text: 'Running', cls: 'state-running' }
      : proc.last_exit === 'exit code 0'
        ? { text: 'Finished', cls: 'state-stopped' }
        : failed
          ? { text: `Exited, ${proc.last_exit}`, cls: 'state-failed' }
          : { text: 'Stopped', cls: 'state-stopped' },
  )

  // Number of lit segments in the 10-segment CPU meter
  const lit = $derived(proc.running ? Math.round(Math.min(proc.cpu, 100) / 10) : 0)

  async function act(action: 'start' | 'stop') {
    busy = true
    note = action === 'start' ? 'Starting…' : 'Stopping…'
    try {
      const res = await api(`/api/${action}/${encodeURIComponent(proc.name)}`, { method: 'POST' })

      showNote(await res.text());
    } catch {
      showNote('No answer from the server. Check that weber is running.');
    }
    busy = false
    onchange()
  }

  // Stop the note timer if the card is removed
  $effect(() => () => clearTimeout(noteTimer))

  async function remove(): Promise<string | null> {
    try {
      const res = await api(`/api/processes/${encodeURIComponent(proc.name)}`, {method: 'DELETE'});
      if (!res.ok) return (await res.json().catch(() => null))?.error ?? `The server answered: ${res.status}.`
      onchange();
      return null;
    } catch {
      return 'No answer from the server. Check the service is running.';
    }
  }

  function showNote(text: string) {
    note = text;
    clearTimeout(noteTimer);
    noteTimer = setTimeout(() => (note = ''), 6000);
  }
</script>

<article class="window proc">
  <div class="titlebar">
    <span class="led" class:on={proc.running} class:err={failed}></span>
    <h2 class="titlebar-text">{proc.name}</h2>
    <!-- Caption buttons -->
    <ToolButton title="Edit" label={proc.name} icon="edit" onclick={() => (dialog = 'edit')} />
    <ToolButton title="Delete" label={proc.name} icon="trash" onclick={() => (dialog = 'delete')} />

  </div>

  <div class="window-body">
    <dl class="stats">
      <dt>Status</dt>
      <dd class={status.cls}>{status.text}</dd>

      <dt>PID</dt>
      <dd>{proc.pid ?? '–'}</dd>

      <dt>CPU</dt>
      <dd>
        <span class="meter" aria-hidden="true">
          {#each { length: 10 }, i}
            <i class:lit={i < lit} class:hot={i < lit && i >= 7}></i>
          {/each}
        </span>{proc.running ? proc.cpu.toFixed(1) + '%' : '–'}
      </dd>

      <dt>Memory</dt>
      <dd>{proc.running ? formatBytes(proc.mem_bytes) : '–'}</dd>

      <dt>Command</dt>
      <dd class="cmd" title={command}>{command}</dd>
    </dl>

    <div class="actions">
      <button class="btn" type="button" disabled={busy || proc.running} onclick={() => act('start')}>
        <Icon name="play" />Start
      </button>
      <button class="btn" type="button" disabled={busy || !proc.running} onclick={() => act('stop')}>
        <Icon name="stop" />Stop
      </button>
      <button class="btn" type="button" onclick={(e) => onlogs(e.currentTarget)}>
        <Icon name="logs" />Logs
        {#if proc.err_count > 0}
          <span class="badge" title="{proc.err_count} error lines since the last start">
            {proc.err_count > 99 ? '99+' : proc.err_count}
          </span>
        {/if}
      </button>
    </div>

    <p class="note" aria-live="polite">{note}</p>
  </div>
</article>

<!-- Dialog -->
{#if dialog === 'edit'}
    <ProcessForm onclose={() => (dialog = null)} onsaved={()=> { showNote('Saved.'); onchange();}} initial={proc.config} running={proc.running}/>
{:else if dialog==='delete'}
    <ConfirmDialog
        title="Delete {proc.name}"
        confirmLabel={proc.running ? "Stop and delete" : "Delete"}
        danger
        onconfirm={remove}
        onclose={() => (dialog = null)}
    />
{/if}
