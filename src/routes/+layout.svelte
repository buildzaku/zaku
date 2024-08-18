<script lang="ts">
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
            // console.log("Invoking `create_space`");
            // const createSpace = await invoke("create_space", { path: "yeoolooo" });
            // console.log("create_space result:");
            // console.log({ createSpace });

            await activeSpace.synchronize();
            console.log({ activeSpace: $activeSpace });

            if ($activeSpace !== null) {
                console.log("FOUND!!");
                await goto("/space");
            } else {
                console.log("NOT FOUND!!");
            }

            await invoke("show_main_window");
        } catch (error) {
            console.error("unable to invoke");
            console.error(error);
        }
    };

    onMount(async () => {
        document.addEventListener("contextmenu", disableContextMenu);

        await synchronize();
    });

    onDestroy(() => {
        document.removeEventListener("contextmenu", disableContextMenu);
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
