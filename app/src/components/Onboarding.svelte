<!-- First run: how do you want to play? Signing in here is optional. -->
<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { api, errorText, type Foreign } from "../lib/api";
  import Block from "./Block.svelte";
  import { materialFor } from "../lib/theme";

  let found = $state<Foreign[]>([]);
  let name = $state("");
  let offline = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    api.importScan().then((f) => (found = f)).catch(() => {});
  });

  function finish() {
    if (app.settings) void app.saveSettings({ ...app.settings, onboarded: true });
  }

  function microsoft() {
    finish();
    app.dialog = "msa";
  }

  function skip() {
    finish();
    app.toast("You'll sign in through the Minecraft Launcher when you press Play");
  }

  async function addOffline(e: SubmitEvent) {
    e.preventDefault();
    try {
      app.accounts = await api.addOfflineAccount(name.trim());
      finish();
    } catch (err) {
      error = errorText(err);
    }
  }
</script>

<div class="screen" role="dialog" aria-modal="true" aria-labelledby="welcome-title">
  <div class="inner">
    <Block material={materialFor("amethyst")} key="welcome" size={120} />
    <h1 class="display" id="welcome-title">Welcome to Elysium</h1>
    <p class="subhead muted">Choose how you'd like to play. You can change this any time in Accounts.</p>

    <div class="choices">
      <button class="choice state" onclick={microsoft}>
        <span class="icon">account_circle</span>
        <span class="title-m">Sign in with Microsoft</span>
        <span class="body-m muted">Play online and use your skin right from Elysium.</span>
      </button>
      <button class="choice state" onclick={skip}>
        <span class="icon">open_in_new</span>
        <span class="title-m">Use the Minecraft Launcher</span>
        <span class="body-m muted">Skip signing in here. Elysium gets everything ready, then the Minecraft Launcher opens for you to sign in and play.</span>
      </button>
    </div>

    {#if offline}
      <form class="offline" onsubmit={addOffline}>
        <label class="field" class:invalid={!!error}>
          <input bind:value={name} placeholder=" " maxlength="16" spellcheck="false" autocomplete="off" />
          <span class="float">Username</span>
          {#if error}<span class="support">{error}</span>{/if}
        </label>
        <button class="btn filled state" type="submit" disabled={!/^[A-Za-z0-9_]{3,16}$/.test(name.trim())}>Continue</button>
      </form>
    {:else}
      <button class="btn text state" onclick={() => (offline = true)}>Play offline instead</button>
    {/if}

    {#if found.length}
      <div class="import group">
        <span class="icon">move_to_inbox</span>
        <span class="body-m">Found {found.length} instance{found.length === 1 ? "" : "s"} from other launchers.</span>
        <button class="btn tonal state" onclick={() => { finish(); app.dialog = "import"; }}>Bring them over</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .screen {
    position: fixed;
    inset: 0;
    z-index: 35;
    background: var(--bg);
    display: grid;
    place-items: center;
    overflow-y: auto;
    animation: fade 400ms var(--ease-out);
  }
  .inner { width: min(760px, calc(100vw - 64px)); padding: 48px 0; display: flex; flex-direction: column; align-items: center; text-align: center; gap: 16px; }
  .display { font-size: 64px; margin-top: 8px; }
  .subhead { max-width: 40ch; margin-bottom: 16px; }
  .choices { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; width: 100%; }
  .choice {
    appearance: none;
    border: 0;
    text-align: left;
    font: inherit;
    color: var(--label);
    background: var(--card);
    border-radius: 24px;
    padding: 24px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    box-shadow: var(--shadow-card);
    cursor: pointer;
  }
  .choice .icon { font-size: 32px; color: var(--accent); margin-bottom: 4px; }
  .offline { display: flex; gap: 12px; align-items: flex-start; width: min(460px, 100%); text-align: left; }
  .offline .field { flex: 1; }
  .offline .btn { height: 56px; }
  .import { display: flex; align-items: center; gap: 12px; padding: 12px 12px 12px 18px; margin-top: 16px; text-align: left; }
  .import .icon { color: var(--accent); }
  @media (max-width: 720px) { .choices { grid-template-columns: 1fr; } }
  @keyframes fade { from { opacity: 0; } }
</style>
