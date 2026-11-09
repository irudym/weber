<script lang="ts">
  import Icon from './Icon.svelte';
  import Modal from './Modal.svelte';

  let { onclose }: { onclose: () => void } = $props()
</script>

<Modal title="Add process" icon="logs" onclose={onclose}>
    <div class="window-body">
        <div class="stats">
            <form class="" method="post" action="/add">
                <label class="field">
                  <span class="field-label">Name</span>
                  <input class="input" name="name"
                         autocapitalize="none" autocorrect="off" spellcheck="false" required>
                </label>
                <label class="field">
                  <span class="field-label">Program</span>
                  <input class="input" name="program"
                         autocapitalize="none" autocorrect="off" spellcheck="false" required>
                </label>
                <label class="field">
                  <span class="field-label">Args</span>
                  <input class="input" name="args"
                         autocapitalize="none" autocorrect="off" spellcheck="false">
                </label>
                <label class="field">
                  <span class="field-label">Directory</span>
                  <input class="input" name="directory"
                         autocapitalize="none" autocorrect="off" spellcheck="false" required>
                </label>
                <label class="field">
                  <span class="field-label">Stop timeout</span>
                  <input class="input" name="timeout"
                         autocapitalize="none" autocorrect="off" spellcheck="false" required>
                </label>
                <button class="btn btn-primary" type="submit">Add</button>
            </form>
        </div>
    </div>
</Modal>
