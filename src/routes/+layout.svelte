<script lang="ts">
    import { dev } from "$app/environment";
    import { ModeWatcher } from "mode-watcher";

    import "../app.css";
    import { TitleBar } from "$lib/components/title-bar";
    import { invoke } from "@tauri-apps/api/core";

    import { onDestroy, onMount } from "svelte";
    import { goto } from "$app/navigation";
    import { activeSpace } from "$lib/store";

    let welcomeMessage: string | null = null;

    const disableContextMenu = (event: MouseEvent) => {
        event.preventDefault();
    };

    const synchronize = async () => {
        try {
            await activeSpace.synchronize();

            if ($activeSpace !== null) {
                await goto("/space");
            }

            await invoke("show_main_window");
        } catch (err) {
            console.error(err);
        }
    };

    onMount(async () => {
        if (!dev) {
            document.addEventListener("contextmenu", disableContextMenu);
        }

        await synchronize();
    });

    onDestroy(() => {
        if (!dev) {
            document.removeEventListener("contextmenu", disableContextMenu);
        }
    });

    $: $activeSpace === null, goto("/");
</script>

<ModeWatcher defaultMode="dark" track={false} />
<main class="bg-background">
    <TitleBar class="h-8" />
    <div id="application" class="h-[calc(100dvh-2rem-1px)]">
        <slot />
    </div>
</main>
