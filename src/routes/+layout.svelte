<script lang="ts">
    import { ModeWatcher } from "mode-watcher";

    import "../app.css";
    import { TitleBar } from "$lib/components/title-bar";
    import { invoke } from "@tauri-apps/api/core";
    import { appDataDir } from "@tauri-apps/api/path";
    import { onDestroy, onMount } from "svelte";
    import { goto } from "$app/navigation";
    // import { initializeCurrentWorkspace, activeWorkspace } from "$lib/store";
    import { emit, listen, type UnlistenFn, once } from "@tauri-apps/api/event";
    import { getCurrent, WebviewWindow } from "@tauri-apps/api/webviewWindow";
    import { activeWorkspace } from "$lib/store";

    let welcomeMessage: string | null = null;
    // let appWebviewUnlisten: UnlistenFn;
    // let unlisten: UnlistenFn;
    // let unlisten2: UnlistenFn;

    // const unlisten = await listen("www_event", event => {
    //     console.log("Received event:", event.payload);
    //     // Handle the event as needed
    // });
    const initialize = async () => {
        try {
            // console.log("Invoking `create_workspace`");
            // const createWorkspace = await invoke("create_workspace", { path: "yeoolooo" });
            // console.log("create_workspace result:");
            // console.log({ createWorkspace });

            await activeWorkspace.initialize();
            console.log({ activeWorkspace: $activeWorkspace });

            if ($activeWorkspace === null) {
                await goto("/welcome");
            } else {
                console.log("FOUND!!");
                // welcomeMessage = await invoke("get_app_state");
            }
        } catch (error) {
            console.error("unable to invoke");
            console.error(error);
        }
    };

    onMount(async () => {
        initialize();

        // console.log("Component mounted, initializing...");
        // await once("tauri://window-created", () => console.log("uri://window-created"));
        // await emit("rust-event", { yo: "epepepoeoepeoepeo" });
        // await emit("active_workspace", { yo: "epepepoeoepeoepeo" });
        // await emit("active_workspace_w", { yo: "epepepoeoepeoepeo" });
        // const appWebview = getCurrent();

        // appWebviewUnlisten = await WebviewWindow.getCurrent().listen(
        //     "active_workspace_w",
        //     event => {
        //         console.log("Received active_workspace event:", event);
        //         console.log("Event payload:", event.payload);
        //     },
        // );

        // unlisten = await listen("active_workspace", event => {
        //     console.log("Received active_workspace event:", event);
        //     console.log("Event payload:", event.payload);
        // });

        // unlisten2 = await listen("rust-event", event => {
        //     console.log("payload received!!!", event.payload);
        // });

        // console.log("listeners", appWebview.listeners);
    });

    // onDestroy(() => {
    //     console.log("Component unmounting, cleaning up listener");

    //     appWebviewUnlisten();
    //     unlisten();
    //     unlisten2();
    // });

    // $: $activeWorkspace === null, initialize();
</script>

<ModeWatcher defaultMode="dark" track={false} />
<main class="h-dvh w-dvw rounded-lg border-[1px] bg-background">
    <TitleBar class="h-8" />
    <div id="application" class="h-[calc(100dvh-2rem-1px)]">
        <slot />
    </div>
</main>
