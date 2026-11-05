<!-- Microsoft sign-in with a device code, like signing in to a TV app. -->
<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { api, errorText } from "../lib/api";

  let code = $state<{ user_code: string; verification_uri: string } | null>(null);
  let error = $state<string | null>(null);
  let copied = $state(false);

  $effect(() => {
    let stop = false;
    api.msaStart()
      .then((c) => {
        code = c;
        void api.openUrl(c.verification_uri).catch(() => {});
      })
      .catch((e) => (error = errorText(e)));
    const t = setInterval(async () => {
      if (stop) return;
      const s = await api.msaStatus().catch(() => null);
      if (s?.kind === "done") {
        app.accounts = s.accounts;
        app.toast(`Signed in as ${app.activeAccount?.username ?? "your account"}`);
        app.dialog = null;
      } else if (s?.kind === "failed") {
        error = s.message;
      }
    }, 1500);
    return () => {
      stop = true;
      clearInterval(t);
    };
  });

  function close() {
    void api.msaCancel();
    app.dialog = null;
  }

  async function copy() {
    if (!code) return;
    await navigator.clipboard.writeText(code.user_code);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && close()} />
<div class="scrim" role="presentation" onclick={close}></div>
<div class="dialog" role="dialog" aria-modal="true" aria-labelledby="msa-title">
  <h2 class="title-l" id="msa-title">Sign in with Microsoft</h2>
  {#if error}
    <p class="body-m error">{error}</p>
    {#if error.includes("client ID")}
      <button class="btn tonal state" onclick={() => { close(); app.route = { name: "settings" }; }}>Open Settings</button>
    {/if}
  {:else if !code}
    <p class="body-m muted">Contacting Microsoft</p>
  {:else}
    <p class="body-m muted">A browser window opened. Sign in there and enter this code:</p>
    <button class="code" onclick={copy} title="Copy code">{code.user_code}</button>
    <p class="body-s muted">{copied ? "Copied" : "Click the code to copy it."} Didn't open? Go to {code.verification_uri}.</p>
    <div class="waiting"><span class="spinner"></span><span class="body-m muted">Waiting for you to finish signing in</span></div>
  {/if}
  <div class="buttons">
    {#if code}<button class="btn text state" onclick={() => api.openUrl(code!.verification_uri)}>Open again</button>{/if}
    <button class="btn tonal state" onclick={close}>Cancel</button>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: rgb(0 0 0 / 0.32); z-index: 40; }
  .dialog {
    position: fixed; z-index: 41; left: 50%; top: 50%; transform: translate(-50%, -50%);
    width: min(440px, calc(100vw - 48px)); padding: 28px; border-radius: 28px;
    background: var(--card); box-shadow: var(--shadow-float);
    display: flex; flex-direction: column; gap: 14px; align-items: stretch;
  }
  .code {
    appearance: none; border: 0; padding: 18px; border-radius: 16px; background: var(--fill);
    font: inherit; font-size: 34px; font-weight: 700; letter-spacing: 0.12em; color: var(--label);
    font-variant-numeric: tabular-nums; cursor: copy; user-select: all;
  }
  .waiting { display: flex; align-items: center; gap: 10px; }
  .spinner { width: 18px; height: 18px; border-radius: 50%; border: 2.5px solid var(--fill-strong); border-top-color: var(--accent); animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .error { color: #ff3b30; }
  .buttons { display: flex; justify-content: flex-end; gap: 8px; margin-top: 4px; }
</style>
