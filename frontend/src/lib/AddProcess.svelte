<script lang="ts">
  import Modal from './Modal.svelte';
  import { api } from './api'

  let { onclose, onadded }: { onclose: () => void; onadded: () => void  } = $props()

  // The form's fields, bound to the inputs below
  let form = $state({ name: '', program: '', args: '', dir: '', stop_timeout_secs: 10 })
  let error = $state('')
  let saving = $state(false)

  async function submit(e: SubmitEvent) {
    e.preventDefault() // stay in the app instead of navigating to a new page
    error = ''
    saving = true
    try {
      const res = await api('/api/processes', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(form),
      })
      if (res.ok) {

        onadded() // let the dashboard reload the list
        onclose()
      } else {
        // 400/409 from the handler: { "error": "..." }
        error = (await res.json()).error ?? `The server answered ${res.status}.`
      }
    } catch {
      error = 'No answer from the server. Check that weber is running.'
    } finally {
      saving = false
    }
  }
</script>

<Modal title="Add process" icon="logs" onclose={onclose} >
    <form class="window-body" onsubmit={submit}>
        {#if error}
            <p class="alert" role="alert">{error}</p>
        {/if}

        <label class="field">
            <span class="field-label">Name</span>
            <input class="input" bind:value={form.name} pattern="[A-Za-z0-9_\-]+"
                    autocapitalize="none" autocorrect="off" spellcheck="false" required>
        </label>
        <label class="field">
            <span class="field-label">Program</span>
            <input class="input" bind:value={form.program}
                    autocapitalize="none" autocorrect="off" spellcheck="false" required>
        </label>
        <label class="field">
            <span class="field-label">Arguments</span>
            <input class="input" bind:value={form.args}
                    autocapitalize="none" autocorrect="off" spellcheck="false">
        </label>
        <label class="field">
            <span class="field-label">Working directory</span>
            <input class="input" bind:value={form.dir}
                    autocapitalize="none" autocorrect="off" spellcheck="false">
        </label>
        <label class="field">
            <span class="field-label">Stop timeout (seconds)</span>
            <input class="input" bind:value={form.stop_timeout_secs}
                    type="number" inputmode="numeric" min="1" max="600" required>
        </label>
        <button class="btn btn-primary" type="submit" disabled={saving}>
            {saving ? 'Adding...' : 'Add process'}
        </button>
    </form>
</Modal>
