<script lang="ts">
    import { untrack } from "svelte";
    import Modal from './Modal.svelte';
    import { api, joinArgs, type ProcConfig} from './api';


    let {
      initial,
      running = false,
      onclose,
      onsaved,
    } : {
      initial?: ProcConfig,
      running?: boolean,
      onclose: () => void,
      onsaved: () => void,
    } = $props();

    const start = untrack(() => initial);
    const editing = start !== undefined;

    let form = $state({
      name: start?.name ?? '',
      program: start?.program ?? '',
      args: start ? joinArgs(start.args) : '',
      dir: start?.dir ?? '',
      stop_timeout_secs: start?.stop_timeout_secs ?? 10,
    });

    let error = $state('');
    let saving = $state(false);

    async function submit(e: SubmitEvent) {
      e.preventDefault();
      error = '';
      saving = true;

      try {
        const res = await api(
          editing ? `/api/processes/${encodeURIComponent(form.name)}` : '/api/processes',
          {
            method: editing ? 'PUT' : 'POST',
            headers: {'Content-Type' : 'application/json'},
            body: JSON.stringify(form),
          },
        );
        if (res.ok) {
          onsaved();
          onclose();
        } else {
          error = (await res.json().catch(() => null))?.error ?? `The server answered ${res.status}.`;
        }
      } catch {
        error = 'No answer from the server, Check that the service is running.';
      } finally {
        saving = false;
      }
    }
</script>

<Modal title={editing ? `Edit ${form.name}` : 'Add process'} icon={editing ? 'edit' : 'add'} size="compact" {onclose}>
  <form class="dialog-content" onsubmit={submit}>
    {#if error}
      <p class="alert" role="alert">{error}</p>
    {/if}

    {#if editing && running}
      <p class="hint">It's running now. Changes apply the next time you start it.</p>
    {/if}

    <label class="field">
      <span class="field-label">Name</span>
      <!-- The name is the process's id in URLs and logs, so it can't change after adding -->
      <input class="input" bind:value={form.name} readonly={editing} required maxlength="32"
             pattern="[A-Za-z0-9_\-]+" placeholder="minecraft"
             autocapitalize="none" autocorrect="off" spellcheck="false" />
    </label>

    <label class="field">
      <span class="field-label">Program</span>
      <input class="input" bind:value={form.program} required placeholder="/usr/bin/java"
             autocapitalize="none" autocorrect="off" spellcheck="false" />
    </label>

    <label class="field">
      <span class="field-label">Arguments</span>
      <input class="input" bind:value={form.args} placeholder={'-Xmx1G -jar paper.jar --nogui'}
             autocapitalize="none" autocorrect="off" spellcheck="false" />
    </label>

    <label class="field">
      <span class="field-label">Working directory</span>
      <input class="input" bind:value={form.dir} placeholder="/opt/minecraft/server"
             autocapitalize="none" autocorrect="off" spellcheck="false" />
    </label>

    <label class="field">
      <span class="field-label">Stop timeout (seconds)</span>
      <input class="input" type="number" inputmode="numeric" min="1" max="600"
             bind:value={form.stop_timeout_secs} required />
    </label>

    <div class="dialog-actions">
      <button class="btn" type="button" onclick={onclose} disabled={saving}>Cancel</button>
      <button class="btn btn-primary" type="submit" disabled={saving}>
        {saving ? 'Saving…' : editing ? 'Save changes' : 'Add process'}
      </button>
    </div>
  </form>
</Modal>
