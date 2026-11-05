<!-- Live game log with a virtualised list so 20k lines stay fast. -->
<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { api, type LogEntry } from "../lib/api";

  let { id }: { id: string } = $props();

  const ROW = 20;
  let lines = $state<LogEntry[]>([]);
  let follow = $state(true);
  let scroller = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let viewport = $state(600);
  let next = 0;

  const inst = $derived(app.instances.find((i) => i.id === id));
  const seq = $derived(app.sessions[id]?.log_seq ?? 0);

  async function pull() {
    if (seq < next) {
      // A new session (or a clear) restarted the numbering.
      lines = [];
      next = 0;
    }
    if (seq === next && lines.length) return;
    const chunk = await api.logs(id, next);
    if (chunk.length === 0) return;
    lines = lines.concat(chunk).slice(-20000);
    next = chunk[chunk.length - 1]!.seq + 1;
    if (follow) queueMicrotask(() => scroller && (scroller.scrollTop = scroller.scrollHeight));
  }

  $effect(() => {
    void seq;
    void pull();
  });
  $effect(() => {
    // Keep pulling while the page is open, in case the session poller is idle.
    const t = setInterval(() => void pull(), 1000);
    return () => clearInterval(t);
  });

  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - 20));
  const last = $derived(Math.min(lines.length, Math.ceil((scrollTop + viewport) / ROW) + 20));

  function onscroll() {
    if (!scroller) return;
    scrollTop = scroller.scrollTop;
    const atBottom = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < ROW * 2;
    follow = atBottom;
  }

  async function clear() {
    await api.clearLogs(id);
    lines = [];
  }

  async function copy() {
    await navigator.clipboard.writeText(lines.map((l) => l.text).join("\n"));
    app.toast("Copied the log");
  }
</script>

<div class="logs">
  <header>
    <button class="icon-btn state" aria-label="Back" onclick={() => (app.route = { name: "home" })}>
      <span class="icon">arrow_back</span>
    </button>
    <div class="title">
      <h1 class="headline log-title">{inst?.name ?? "Game"} log</h1>
      <p class="body-m muted">{lines.length.toLocaleString()} lines from the latest session</p>
    </div>
    <label class="follow">
      <span class="label-l">Follow</span>
      <input
        class="switch"
        type="checkbox"
        bind:checked={follow}
        onchange={() => follow && scroller && (scroller.scrollTop = scroller.scrollHeight)}
      />
    </label>
    <button class="icon-btn state" title="Copy log" aria-label="Copy log" disabled={!lines.length} onclick={copy}><span class="icon">content_copy</span></button>
    <button class="icon-btn state" title="Clear" aria-label="Clear log" disabled={!lines.length} onclick={clear}><span class="icon">delete_sweep</span></button>
    <button class="icon-btn state" title="Open logs folder" aria-label="Open logs folder" onclick={() => api.openFolder("logs", id).catch((e) => app.toast(String(e)))}><span class="icon">folder_open</span></button>
  </header>

  <div class="console" bind:this={scroller} bind:clientHeight={viewport} {onscroll}>
    {#if lines.length === 0}
      <p class="empty">Nothing here yet. Press Play and the game's output shows up as it happens.</p>
    {:else}
      <div class="spacer" style:height="{lines.length * ROW}px">
        <div class="rows" style:transform="translateY({first * ROW}px)">
          {#each lines.slice(first, last) as l (l.seq)}
            <div class="line {l.level} {l.source}">{l.text}</div>
          {/each}
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .logs { height: 100%; display: flex; flex-direction: column; padding: 16px 40px 32px 8px; gap: 16px; }
  header { display: flex; align-items: center; gap: 8px; }
  .title { flex: 1; margin-left: 8px; }
  .log-title { font-size: 30px; line-height: 36px; }
  .follow { display: flex; align-items: center; gap: 12px; margin-right: 12px; cursor: pointer; }
  .console {
    flex: 1;
    overflow: auto;
    border-radius: 20px;
    background: #1c1c1e;
    color: #e5e5ea;
    padding: 20px 24px;
    font-family: var(--mono);
    font-size: 12.5px;
    user-select: text;
    cursor: text;
  }
  .spacer { position: relative; min-width: max-content; }
  .rows { position: absolute; left: 0; top: 0; right: 0; }
  .line { height: 20px; line-height: 20px; white-space: pre; }
  .line.launcher { color: #64d2ff; }
  .line.warn { color: #ffd60a; }
  .line.error { color: #ff6961; }
  .line.debug { opacity: 0.6; }
  .empty { font-family: var(--font); font-size: 15px; opacity: 0.7; margin: 0; }
</style>
