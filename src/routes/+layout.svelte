<script lang="ts">
    import { onDestroy, onMount } from "svelte";
    import { dev } from "$app/environment";
    import { goto } from "$app/navigation";
    import { page } from "$app/stores";
    import { invoke } from "@tauri-apps/api/core";
    import { ModeWatcher } from "mode-watcher";

    import "../app.css";
    import { TitleBar } from "$lib/components/title-bar";
    import { activeSpace } from "$lib/store";

    const disableContextMenu = (event: MouseEvent) => {
        event.preventDefault();
    };

    const initialize = async () => {
        await activeSpace.synchronize();

        console.log("$activeSpace", $activeSpace);

        if ($activeSpace !== null) {
            await goto("/space");
        } else if ($page.url.pathname !== "/") {
            await goto("/");
        }

        await invoke("show_main_window");
    };

    onMount(async () => {
        if (!dev) {
            document.addEventListener("contextmenu", disableContextMenu);
        }

        await initialize();
    });

    onDestroy(() => {
        if (!dev) {
            document.removeEventListener("contextmenu", disableContextMenu);
        }
    });

    $: if ($activeSpace === null) {
        goto("/");
    }
</script>

<ModeWatcher defaultMode="dark" track={false} />
<main class="bg-background">
    <TitleBar class="h-8" />
    <div class="h-[calc(100dvh-2rem-1px)]">
        <slot />
    </div>
</main>
