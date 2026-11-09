<script lang="ts">
  import Icon from './Icon.svelte';
  import Modal from './Modal.svelte';
  import { api, formatTime, type LogLine } from './api';

  let { name, onclose }: { name: string; onclose: () => void } = $props()

  const MAX_LINES = 2000

  let lines = $state<LogLine[]>([])
  let tab = $state<'all' | 'err'>('all')
  let follow = $state(true)
  let term = $state<HTMLDivElement>()
  let since = 0 // last line number received; plain variable, not shown anywhere

  const shown = $derived(tab === 'all' ? lines : lines.filter((l) => l.stream === 'err'))
  const errCount = $derived(lines.filter((l) => l.stream === 'err').length)

  async function poll() {
    try {
      const res = await api(`/api/logs/${encodeURIComponent(name)}?since=${since}`)
      const data: { lines: LogLine[] } = await res.json()
      if (data.lines.length === 0) return
      since = data.lines[data.lines.length - 1].seq
      const all = lines.concat(data.lines)
      lines = all.length > MAX_LINES ? all.slice(-MAX_LINES) : all
    } catch {
      // try again on the next tick
    }
  }


  // After the visible lines change (or "follow" is switched on), jump to the bottom.
  // Effects run after the DOM is updated, so scrollHeight already includes new lines.
  $effect(() => {
    if (shown && follow && term) term.scrollTop = term.scrollHeight
  })

  // Scrolling up pauses following; scrolling back to the bottom resumes it.
  function onScroll() {
    if (term) follow = term.scrollHeight - term.scrollTop - term.clientHeight < 24
  }

</script>

<!-- Clicking the dimmed area closes the viewer; keyboard users have Escape and the close button -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<Modal title="Logs {name}" icon="logs" onclose={onclose}>
    <div class="tabs" role="tablist">
      <button class="tab" type="button" role="tab" aria-selected={tab === 'all'} onclick={() => (tab = 'all')}>
        All output
      </button>
      <button class="tab" type="button" role="tab" aria-selected={tab === 'err'} onclick={() => (tab = 'err')}>
        Errors {errCount ? `(${errCount})` : ''}
      </button>
    </div>

    <!-- tabindex: lets keyboard users focus the log and scroll it with the arrow keys -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="terminal" role="log" tabindex="0" bind:this={term} onscroll={onScroll}>
      <!-- keyed by seq: Svelte only adds the new lines to the page -->
      {#each shown as line (line.seq)}
        <div class="line {line.stream}">
          <time>{formatTime(line.ts_ms)}</time>
          <span>{line.text}</span>
        </div>
      {:else}
        <p class="terminal-empty">
          {tab === 'err'
            ? 'No errors. Lines the process writes to stderr appear here.'
            : 'No output yet. Start the process to see what it prints.'}
        </p>
      {/each}
    </div>

    <div class="log-footer">
      <label class="check"><input type="checkbox" bind:checked={follow} />Follow new lines</label>
      <span class="line-count">{shown.length === 1 ? '1 line' : `${shown.length} lines`}</span>
    </div>
</Modal>
