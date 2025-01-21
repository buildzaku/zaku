<script lang="ts">
    import { onDestroy, onMount } from "svelte";
    import type { Snippet } from "svelte";
    import { dev } from "$app/environment";
    import { goto } from "$app/navigation";
    import { page } from "$app/state";
    import { invoke } from "@tauri-apps/api/core";
    import { ModeWatcher } from "mode-watcher";

    import "../app.css";
    import { Toaster } from "$lib/components/primitives/sonner";
    import { TitleBar } from "$lib/components/title-bar";
    import { zakuState } from "$lib/state.svelte";

    let { children }: { children: Snippet } = $props();

    const disableContextMenu = (event: MouseEvent) => {
        event.preventDefault();
    };

    onMount(async () => {
        if (!dev) {
            document.addEventListener("contextmenu", disableContextMenu);
        }
        await zakuState.synchronize();

        if (zakuState.activeSpace !== null) {
            await goto("/space");
        } else if (page.url.pathname !== "/") {
            await goto("/");
        }

        await invoke("show_main_window");
    });

    $effect(() => {
        if (zakuState.activeSpace === null) {
            goto("/");
        }
    });

    onDestroy(() => {
        if (!dev) {
            document.removeEventListener("contextmenu", disableContextMenu);
        }
    });
</script>

<ModeWatcher defaultMode="dark" track={false} />
<Toaster />
<main class="bg-background">
    <TitleBar class="h-[36px]" />
    <div class="h-[calc(100dvh-36px)]">
        {@render children()}
    </div>
</main>
