<script lang="ts">
    import { ModeWatcher } from "mode-watcher";

    import "../app.css";
    import { TitleBar } from "$lib/components/title-bar";
    import { invoke } from "@tauri-apps/api/core";
    import { appDataDir } from "@tauri-apps/api/path";
    import { onDestroy, onMount } from "svelte";
    import { goto } from "$app/navigation";
    import { activeWorkspace } from "$lib/store";

    let welcomeMessage: string | null = null;

    const synchronize = async () => {
        try {
            // console.log("Invoking `create_workspace`");
            // const createWorkspace = await invoke("create_workspace", { path: "yeoolooo" });
            // console.log("create_workspace result:");
            // console.log({ createWorkspace });

            await activeWorkspace.synchronize();
            console.log({ activeWorkspace: $activeWorkspace });

            if ($activeWorkspace !== null) {
                console.log("FOUND!!");
                await goto("/workspace");
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
        synchronize();
    });

    // onDestroy(() => {
    //     console.log("Component unmounting, cleaning up listener");

    //     appWebviewUnlisten();
    //     unlisten();
    //     unlisten2();
    // });

    $: $activeWorkspace === null, goto("/");
</script>

<ModeWatcher defaultMode="dark" track={false} />
<main class="bg-background">
    <TitleBar class="h-8" />
    <div id="application" class="h-[calc(100dvh-2rem-1px)]">
        <slot />
    </div>
</main>
