<!-- The right-click menu. Opened through app.openMenu; closes on choice, Escape or outside click. -->
<script lang="ts">
  import { tick } from "svelte";
  import { app } from "../lib/store.svelte";

  const menu = $derived(app.menu);
  let el = $state<HTMLDivElement>();
  let pos = $state({ x: 0, y: 0 });

  // Keep the menu on screen, opening up or left when there's no room.
  $effect(() => {
    if (!menu) return;
    pos = { x: menu.x, y: menu.y };
    void tick().then(() => {
      if (!el) return;
      const r = el.getBoundingClientRect();
      pos = {
        x: menu.x + r.width > innerWidth - 8 ? Math.max(8, menu.x - r.width) : menu.x,
        y: menu.y + r.height > innerHeight - 8 ? Math.max(8, menu.y - r.height) : menu.y,
      };
      el.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
    });
  });

  function close() {
    app.menu = null;
  }

  function choose(run: () => void) {
    close();
    run();
  }

  function onkeydown(e: KeyboardEvent) {
    if (!menu || !el) return;
    if (e.key === "Escape") {
      e.preventDefault();
      close();
      return;
    }
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp" && e.key !== "Home" && e.key !== "End") return;
    e.preventDefault();
    const items = [...el.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
    const i = items.indexOf(document.activeElement as HTMLButtonElement);
    const next =
      e.key === "Home" ? 0 : e.key === "End" ? items.length - 1 : e.key === "ArrowDown" ? (i + 1) % items.length : (i - 1 + items.length) % items.length;
    items[next]?.focus();
  }
</script>

<svelte:window {onkeydown} onblur={close} onresize={close} />

{#if menu}
  <div class="catcher" role="presentation" onpointerdown={close} oncontextmenu={(e) => { e.preventDefault(); close(); }}></div>
  <div class="menu" role="menu" tabindex="-1" bind:this={el} style:left="{pos.x}px" style:top="{pos.y}px">
    {#each menu.items as item, n (n)}
      {#if item === "separator"}
        <div class="sep" role="separator"></div>
      {:else}
        <button role="menuitem" class:danger={item.danger} disabled={item.disabled} onclick={() => choose(item.run)}>
          <span class="icon">{item.icon ?? ""}</span>{item.label}
        </button>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .catcher { position: fixed; inset: 0; z-index: 90; }
  .menu {
    position: fixed;
    z-index: 91;
    min-width: 220px;
    padding: 6px;
    border-radius: 14px;
    background: color-mix(in srgb, var(--card) 86%, transparent);
    backdrop-filter: blur(24px) saturate(1.6);
    box-shadow: var(--shadow-float), 0 0 0 0.5px var(--separator);
    display: flex;
    flex-direction: column;
    transform-origin: top left;
    animation: pop 280ms var(--spring);
    transform-origin: top left;
    outline: none;
  }
  :root[data-reduce-transparency="true"] .menu { background: var(--card); }
  @keyframes pop { from { opacity: 0; transform: scale(0.94); } }
  button {
    appearance: none;
    border: 0;
    background: none;
    font: inherit;
    font-size: 14px;
    font-weight: 500;
    color: var(--label);
    height: 32px;
    padding: 0 12px 0 8px;
    border-radius: 8px;
    display: flex;
    align-items: center;
    gap: 10px;
    text-align: left;
    cursor: pointer;
    white-space: nowrap;
  }
  button .icon { font-size: 18px; width: 20px; color: var(--label-2); }
  button:hover:not(:disabled),
  button:focus-visible { background: var(--accent); color: var(--on-accent); outline: none; }
  button:hover:not(:disabled) .icon,
  button:focus-visible .icon { color: inherit; }
  button.danger { color: #ff3b30; }
  button.danger:hover:not(:disabled),
  button.danger:focus-visible { background: #ff3b30; color: #fff; }
  button:disabled { opacity: 0.4; cursor: default; }
  .sep { height: 1px; margin: 5px 8px; background: var(--separator); }
</style>
