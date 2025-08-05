<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import type { Snippet } from "svelte";
  import { dev } from "$app/environment";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import { ModeWatcher, setMode } from "mode-watcher";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import type { UnlistenFn } from "@tauri-apps/api/event";

  import "../app.css";
  import { Toaster } from "$lib/components/primitives/sonner";
  import { TitleBar } from "$lib/components/title-bar";
  import { appStateRx } from "$lib/state.svelte";
  import { commands } from "$lib/bindings";

  let { children }: { children: Snippet } = $props();

  const disableContextMenu = (event: MouseEvent) => {
    event.preventDefault();
  };

  let unlistenThemeChange: UnlistenFn;

  onMount(async () => {
    unlistenThemeChange = await getCurrentWindow().onThemeChanged(({ payload: theme }) => {
      if (appStateRx.space?.settings.theme === "system") {
        setMode(theme);
      }
    });

    if (!dev) {
      document.addEventListener("contextmenu", disableContextMenu);
    }
    await appStateRx.synchronize();

    if (appStateRx.space !== null) {
      await goto("/space");
    } else if (page.url.pathname !== "/") {
      await goto("/");
    }

    // TODO - handle cmd error, figure out?
    await commands.showMainWindow();
  });

  $effect(() => {
    if (appStateRx.space === null) {
      goto("/");
    }
  });

  onDestroy(() => {
    if (!dev) {
      document.removeEventListener("contextmenu", disableContextMenu);
    }

    unlistenThemeChange();
  });
</script>

<ModeWatcher defaultMode="system" track={false} />
<Toaster />
<main class="bg-background">
  <TitleBar class="h-[36px]" />
  <div class="h-[calc(100dvh-36px)]">
    {@render children()}
  </div>
</main>
