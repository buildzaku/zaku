<script lang="ts">
    import { onDestroy, onMount } from "svelte";
    import { dev } from "$app/environment";
    import { goto } from "$app/navigation";
    import { page } from "$app/stores";
    import { invoke } from "@tauri-apps/api/core";
    import { ModeWatcher } from "mode-watcher";

    import { Toaster } from "$lib/components/primitives/sonner";
    import "../app.css";
    import { TitleBar } from "$lib/components/title-bar";
    import { zakuState } from "$lib/store";

    const disableContextMenu = (event: MouseEvent) => {
        event.preventDefault();
    };

    onMount(async () => {
        if (!dev) {
            document.addEventListener("contextmenu", disableContextMenu);
        }

        await zakuState.initialize();

        if ($zakuState.active_space !== null) {
            await goto("/space");
        } else if ($page.url.pathname !== "/") {
            await goto("/");
        }

        await invoke("show_main_window");
    });

    onDestroy(() => {
        if (!dev) {
            document.removeEventListener("contextmenu", disableContextMenu);
        }
    });

    $: if ($zakuState.active_space === null) {
        goto("/");
    }
</script>

<ModeWatcher defaultMode="dark" track={false} />
<Toaster />
<main class="bg-background">
    <TitleBar class="h-8" />
    <div class="h-[calc(100dvh-2rem-1px)]">
        <slot />
    </div>
</main>
