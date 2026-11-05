<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, errorText } from "../lib/api";

  let { path, onchange }: { path: string; onchange: (path: string) => void } = $props();

  let checking = $state(false);
  let result = $state<{ ok: boolean; text: string } | null>(null);

  async function browse() {
    const picked = await open({ title: "Choose a Java executable", multiple: false, directory: false });
    if (typeof picked === "string") {
      result = null;
      onchange(picked);
    }
  }

  async function check() {
    checking = true;
    result = null;
    try {
      const info = await api.probeJava(path);
      result = { ok: true, text: `Java ${info.version} (${info.arch}) works.` };
    } catch (e) {
      result = { ok: false, text: errorText(e) };
    } finally {
      checking = false;
    }
  }
</script>

<div class="java">
  <label class="field" class:invalid={result && !result.ok}>
    <input
      value={path}
      placeholder=" "
      spellcheck="false"
      onchange={(e) => {
        result = null;
        onchange(e.currentTarget.value);
      }}
    />
    <span class="float">Java executable</span>
    {#if result}<span class="support">{result.text}</span>{/if}
  </label>
  <div class="buttons">
    <button class="btn outlined state" onclick={browse}>Browse</button>
    <button class="btn text state" disabled={!path || checking} onclick={check}>{checking ? "Checking" : "Check"}</button>
  </div>
</div>

<style>
  .java { display: grid; gap: 8px; }
  .buttons { display: flex; gap: 8px; }
</style>
