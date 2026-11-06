<!-- Material 3 navigation rail. -->
<script lang="ts">
  import { app } from "../lib/store.svelte";
  import logo from "../assets/logo.svg";

  const items = [
    { route: "home", icon: "home", label: "Home" },
    { route: "discover", icon: "explore", label: "Discover" },
    { route: "accounts", icon: "account_circle", label: "Accounts" },
    { route: "game", icon: "sports_esports", label: "Game" },
    { route: "settings", icon: "settings", label: "Settings" },
  ] as const;

  const current = $derived(app.route.name === "logs" || app.route.name === "content" ? "home" : app.route.name);

  // One highlight that slides to the active tab instead of each tab lighting up on its own.
  let marks = $state<HTMLElement[]>([]);
  const pillY = $derived.by(() => {
    const i = items.findIndex((it) => it.route === current);
    const el = marks[i];
    return el ? el.offsetTop : null;
  });
</script>

<nav class="rail" aria-label="Main">
  <div class="mark" title="Elysium Launcher">
    <img src={logo} alt="" width="44" height="44" />
  </div>
  <ul>
    {#if pillY !== null}<span class="pill" style:transform="translateY({pillY}px)" aria-hidden="true"></span>{/if}
    {#each items as item, i (item.route)}
      {@const active = current === item.route}
      <li bind:this={marks[i]}>
        <button
          class="dest"
          class:active
          aria-current={active ? "page" : undefined}
          onclick={() => (app.route = { name: item.route })}
        >
          <span class="indicator"><span class="icon" class:filled={active}>{item.icon}</span></span>
          <span class="label">{item.label}</span>
        </button>
      </li>
    {/each}
  </ul>
  <div class="spacer"></div>
  <button
    class="avatar state"
    title={app.activeAccount ? `Playing as ${app.activeAccount.username}` : "Not signed in: games open in the Minecraft Launcher"}
    onclick={() => (app.route = { name: "accounts" })}
  >
    {#if app.activeAccount}
      {app.activeAccount.username.slice(0, 1).toUpperCase()}
    {:else}
      <span class="icon">person</span>
    {/if}
  </button>
</nav>

<style>
  .rail {
    width: 88px;
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 20px 0 24px;
    background: var(--chrome);
    backdrop-filter: saturate(180%) blur(20px);
    border-right: 0.5px solid var(--separator);
  }
  .mark { margin-bottom: 32px; }
  .mark img { display: block; filter: drop-shadow(0 4px 8px rgb(0 0 0 / 0.25)); }
  ul { position: relative; list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 10px; }
  .pill {
    position: absolute;
    top: 0;
    left: 14px;
    width: 52px;
    height: 36px;
    border-radius: 12px;
    background: var(--label);
    transition: transform 560ms var(--spring);
    pointer-events: none;
  }
  /* Above the sliding highlight. */
  .dest { position: relative; }
  .dest {
    appearance: none;
    border: 0;
    background: none;
    padding: 0;
    width: 80px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    color: var(--label-2);
    font: inherit;
    cursor: pointer;
  }
  .indicator {
    width: 52px;
    height: 36px;
    border-radius: 12px;
    display: grid;
    place-items: center;
    transition: background-color 250ms var(--ease), color 300ms var(--ease), transform 520ms var(--spring);
  }
  .dest:active .indicator { transform: scale(0.9); transition-duration: 250ms, 300ms, 90ms; }
  .dest:not(.active):hover .indicator { background: var(--fill); }
  .dest.active { color: var(--label); }
  .dest.active .indicator { color: var(--bg); }
  .dest:focus-visible { outline: none; }
  .dest:focus-visible .indicator { outline: 3px solid var(--accent); outline-offset: 2px; }
  .label { font-size: 12px; line-height: 15px; font-weight: 650; }
  .dest.active .label { font-weight: 750; }
  .spacer { flex: 1; }
  .avatar {
    appearance: none;
    border: 0;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--on-accent);
    font: inherit;
    font-size: 19px;
    font-weight: 800;
    display: grid;
    place-items: center;
    cursor: pointer;
  }
</style>
