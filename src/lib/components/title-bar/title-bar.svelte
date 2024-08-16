<script lang="ts">
    import { getCurrent } from "@tauri-apps/api/window";
    import { Cookie, Sun, Moon } from "svelte-radix";
    import { toggleMode } from "mode-watcher";
    import { open } from "@tauri-apps/plugin-dialog";
    import { readDir } from "@tauri-apps/plugin-fs";

    import { ZakuVector } from "$lib/components/vectors";
    import { Button } from "$lib/components/primitives/button";
    import { cn } from "$lib/utils/style";
    // import { persistedStore } from "$lib/store";

    const appWindow = getCurrent();

    async function handleClose() {
        await appWindow.close();
    }

    async function handleMinimize() {
        await appWindow.minimize();
    }

    async function handleFullscreen() {
        const isFullscreen = await appWindow.isFullscreen();
        await appWindow.setFullscreen(!isFullscreen);
    }

    async function handleFolder() {
        // await persistedStore.reset();
        // window.location.reload();
        // Open a selection dialog for directories
        const selected = await open({
            directory: true,
            multiple: false,
        });
        if (Array.isArray(selected)) {
            console.log(1);
            // user selected multiple directories
        } else if (selected === null) {
            console.log(2);
            // user cancelled the selection
        } else {
            console.log(3);
            console.log({ selected: selected });
            const dir = await readDir(selected);
            console.log(dir);
            // user selected a single directory
        }
    }
</script>

<div
    data-tauri-drag-region
    class={cn("flex w-full cursor-default items-center justify-between", $$props["class"])}
>
    <div />
    <div class="flex h-full items-center gap-1.5">
        <Button
            on:click={handleFolder}
            variant="ghost"
            size="icon"
            class="size-6 min-h-6 min-w-6 p-1 hover:bg-accent hover:text-accent-foreground"
        >
            <Cookie size={14} ariaLabel="cookies" />
        </Button>
        <Button
            on:click={toggleMode}
            variant="ghost"
            size="icon"
            class="size-6 min-h-6 min-w-6 p-1 hover:bg-accent hover:text-accent-foreground"
        >
            <Sun size={14} class="block dark:hidden" ariaLabel="light" />
            <Moon size={14} class="hidden dark:block" ariaLabel="dark" />
            <span class="sr-only">Toggle theme</span>
        </Button>
        <div class="px-3">
            <ZakuVector size={11} />
        </div>
    </div>
</div>
