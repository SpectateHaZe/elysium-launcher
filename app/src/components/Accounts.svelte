<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { api, errorText } from "../lib/api";

  let username = $state("");
  let error = $state<string | null>(null);

  const valid = $derived(/^[A-Za-z0-9_]{3,16}$/.test(username.trim()));

  async function add(e: SubmitEvent) {
    e.preventDefault();
    error = null;
    try {
      app.accounts = await api.addOfflineAccount(username.trim());
      app.toast(`Playing as ${username.trim()}`);
      username = "";
    } catch (err) {
      error = errorText(err);
    }
  }

  async function use(uuid: string) {
    try {
      app.accounts = await api.setActiveAccount(uuid);
    } catch (err) {
      app.toast(errorText(err));
    }
  }

  async function remove(uuid: string, name: string) {
    try {
      app.accounts = await api.removeAccount(uuid);
      app.toast(`Removed ${name}`);
    } catch (err) {
      app.toast(errorText(err));
    }
  }
</script>

<div class="scroll"><div class="page">
  <h1 class="headline big">Accounts</h1>
  <p class="body-l muted intro">Choose who you play as. Offline accounts work in singleplayer and on servers that allow them.</p>

  {#if app.accounts.accounts.length}
    <ul class="list group">
      {#each app.accounts.accounts as a (a.uuid)}
        {@const active = a.uuid === app.accounts.active}
        <li class:active>
          <span class="avatar" aria-hidden="true">{a.username.slice(0, 1).toUpperCase()}</span>
          <div class="who">
            <p class="title-m">{a.username}</p>
            <p class="body-m muted">{active ? "Playing as this account" : "Offline account"}</p>
          </div>
          {#if !active}
            <button class="btn tonal state" onclick={() => use(a.uuid)}>Play as {a.username}</button>
          {/if}
          <button class="icon-btn state" title="Remove {a.username}" aria-label="Remove {a.username}" onclick={() => remove(a.uuid, a.username)}>
            <span class="icon">delete</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  <form class="add" onsubmit={add}>
    <h2 class="title-l">Add an offline account</h2>
    <div class="group form-group">
    <div class="inline">
      <label class="field" class:invalid={!!error}>
        <input bind:value={username} placeholder=" " maxlength="16" spellcheck="false" autocomplete="off" />
        <span class="float">Username</span>
        <span class="support">{error ?? "3 to 16 letters, numbers or underscores"}</span>
      </label>
      <button class="btn filled state" type="submit" disabled={!valid}>Add account</button>
    </div>
    </div>
  </form>

  <div class="note">
    <span class="icon">info</span>
    <p class="body-m">Signing in with a Microsoft account, for skins and online servers, is coming in a later update.</p>
  </div>
</div>
</div>

<style>
  .scroll { height: 100%; overflow-y: auto; }
  .page { padding: 56px 40px 72px 8px; max-width: 780px; }
  .big { font-size: 56px; }
  .intro { margin: 10px 0 36px; max-width: 54ch; }
  .list { list-style: none; margin: 0 0 40px; padding: 4px 16px; }
  li { display: flex; align-items: center; gap: 16px; padding: 14px 0; border-top: 1px solid var(--separator); }
  li:first-child { border-top: 0; }
  .avatar {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-size: 20px;
    font-weight: 800;
    background: var(--fill);
    color: var(--label);
  }
  li.active .avatar { background: var(--accent); color: var(--on-accent); }
  .who { flex: 1; min-width: 0; }
  .who .muted { font-size: 13px; }
  li.active .who .muted { color: var(--accent); font-weight: 600; }
  .add h2 { margin: 0 0 12px 4px; }
  .form-group { padding: 20px; }
  .inline { display: flex; gap: 12px; align-items: flex-start; }
  .inline .field { flex: 1; max-width: 360px; }
  .inline .btn { height: 56px; padding: 0 24px; }
  .note { display: flex; gap: 10px; align-items: flex-start; margin: 32px 0 0 4px; color: var(--label-2); max-width: 60ch; }
  .note .icon { font-size: 20px; }
</style>
