<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { api, errorText } from "../lib/api";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import SkinFace from "./SkinFace.svelte";
  import { duration } from "../lib/format";

  // Time played across every instance (sessions run through the Minecraft Launcher aren't timed).
  const total = $derived(app.instances.reduce((n, i) => n + i.playtime_secs, 0));
  const played = $derived(app.instances.filter((i) => i.last_played).length);
  const favourite = $derived(app.instances.reduce<(typeof app.instances)[number] | null>((best, i) => (i.playtime_secs > (best?.playtime_secs ?? 0) ? i : best), null));

  let skinBusy = $state(false);

  async function changeSkin(slim: boolean) {
    const path = await openDialog({ title: "Choose a skin", multiple: false, filters: [{ name: "Skin", extensions: ["png"] }] });
    if (typeof path !== "string") return;
    skinBusy = true;
    try {
      app.accounts = await api.uploadSkin(path, slim);
      app.toast("Skin updated");
    } catch (err) {
      app.toast(errorText(err));
    } finally {
      skinBusy = false;
    }
  }

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
  <p class="body-l muted intro">
    Signing in here is optional. {app.activeAccount ? "" : "Right now games open in the Minecraft Launcher, which signs you in. "}Offline accounts work in singleplayer and on servers that allow them.
  </p>

  <dl class="group totals" aria-label="Your playtime">
    <div><dt>Total playtime</dt><dd>{total ? duration(total) : "None yet"}</dd></div>
    <div><dt>Instances played</dt><dd>{played} of {app.instances.length}</dd></div>
    {#if favourite}<div class="fav"><dt>Most played</dt><dd title={favourite.name}>{favourite.name}</dd></div>{/if}
  </dl>

  {#if app.accounts.accounts.length}
    <ul class="list group">
      {#each app.accounts.accounts as a (a.uuid)}
        {@const active = a.uuid === app.accounts.active}
        <li class:active>
          <SkinFace url={a.skin?.url} name={a.username} size={48} />
          <div class="who">
            <p class="title-m">{a.username}</p>
            <p class="body-m muted">
              {a.kind === "microsoft" ? (a.owns_game ? "Microsoft account" : "Microsoft account without Java Edition") : "Offline account"}{active ? ", playing as this account" : ""}
            </p>
          </div>
          {#if active && a.kind === "microsoft"}
            <button class="btn text state" disabled={skinBusy} onclick={() => changeSkin(a.skin?.variant === "slim")}>Change skin</button>
          {/if}
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

  <section class="ms">
    <h2 class="title-l">Microsoft account (optional)</h2>
    <div class="group ms-row">
      <div class="who">
        <p class="title-m">Play online, with your skin</p>
        <p class="body-m muted">Start games straight from Elysium, with your skin. Needed for Horizon.</p>
      </div>
      <button class="btn filled state" onclick={() => (app.dialog = "msa")}><span class="icon">login</span>Sign in with Microsoft</button>
    </div>
  </section>

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


</div>
</div>

<style>
  .scroll { height: 100%; overflow-y: auto; }
  .page { padding: 56px 40px 72px 8px; max-width: 1280px; }
  .big { font-size: 56px; }
  .intro { margin: 10px 0 36px; max-width: 54ch; }
  .totals { margin: 0 0 40px; padding: 18px 22px; display: flex; flex-wrap: wrap; gap: 16px 44px; }
  .totals div { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .totals .fav { flex: 1; }
  .totals dt { font-size: 13px; line-height: 18px; color: var(--label-2); }
  .totals dd { margin: 0; font-size: 22px; line-height: 28px; font-weight: 600; letter-spacing: -0.015em; font-variant-numeric: tabular-nums; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .list { list-style: none; margin: 0 0 40px; padding: 4px 16px; }
  li { display: flex; align-items: center; gap: 16px; padding: 14px 0; border-top: 1px solid var(--separator); }
  li:first-child { border-top: 0; }
  .who { flex: 1; min-width: 0; }
  .who .muted { font-size: 13px; }
  li.active .who .muted { color: var(--accent); font-weight: 600; }
  .add h2, .ms h2 { margin: 0 0 12px 4px; }
  .ms { margin-bottom: 36px; }
  .ms-row { display: flex; align-items: center; gap: 16px; padding: 18px 20px; }
  .form-group { padding: 20px; }
  .inline { display: flex; gap: 12px; align-items: flex-start; }
  .inline .field { flex: 1; max-width: 360px; }
  .inline .btn { height: 56px; padding: 0 24px; }
</style>
