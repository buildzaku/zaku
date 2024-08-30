<script lang="ts">
    import { onDestroy, onMount } from "svelte";
    import { dev } from "$app/environment";
    import { goto } from "$app/navigation";
    import { page } from "$app/stores";
    import { invoke } from "@tauri-apps/api/core";
    import { ModeWatcher } from "mode-watcher";

    import "../app.css";
    import { TitleBar } from "$lib/components/title-bar";
    import { activeSpace, spaceReferences } from "$lib/store";
    import { emit, listen } from "@tauri-apps/api/event";

    const disableContextMenu = (event: MouseEvent) => {
        event.preventDefault();
    };

    const initialize = async () => {
        // console.log(await invoke("get_zaku_state"));

        await activeSpace.synchronize().catch(e => console.log(e));
        await spaceReferences.synchronize().catch(e => console.log(e));

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
