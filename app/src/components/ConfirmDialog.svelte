<script lang="ts">
  let {
    title,
    body,
    confirm,
    danger = false,
    onconfirm,
    oncancel,
  }: { title: string; body: string; confirm: string; danger?: boolean; onconfirm: () => void; oncancel: () => void } = $props();
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && oncancel()} />

<div class="scrim" onclick={oncancel} role="presentation"></div>
<div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="confirm-title" aria-describedby="confirm-body">
  <h2 class="headline" id="confirm-title">{title}</h2>
  <p class="body-m muted" id="confirm-body">{body}</p>
  <div class="buttons">
    <button class="btn text state" onclick={oncancel}>Cancel</button>
    <button class="btn filled state" class:danger onclick={onconfirm}>{confirm}</button>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: rgb(0 0 0 / 0.32); z-index: 40; }
  .dialog {
    position: fixed;
    z-index: 41;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    width: min(420px, calc(100vw - 48px));
    padding: 24px;
    border-radius: var(--shape-xl);
    background: var(--card);
    box-shadow: var(--shadow-float);
    display: flex;
    flex-direction: column;
    gap: 16px;
    animation: rise 300ms var(--ease-out);
  }
  .headline { font-size: 26px; line-height: 32px; letter-spacing: -0.025em; }
  .buttons { display: flex; justify-content: flex-end; gap: 8px; margin-top: 8px; }
  @keyframes rise { from { opacity: 0; transform: translate(-50%, calc(-50% + 16px)); } }
</style>
