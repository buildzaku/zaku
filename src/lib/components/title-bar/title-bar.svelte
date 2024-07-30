<script lang="ts">
    import { version } from "$app/environment";
    import { getCurrent } from "@tauri-apps/api/window";
    import { Cookie, Sun, Moon } from "svelte-radix";
    import { toggleMode } from "mode-watcher";

    import { ZakuVector } from "$lib/components/vectors";
    import { Button } from "$lib/components/primitives/button";
    import { cn } from "$lib/utils/style";

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
</script>

<div
    data-tauri-drag-region
    class={cn("flex w-full cursor-default items-center justify-between", $$props["class"])}
>
    <div class="ml-3 flex gap-2">
        <button on:click={handleClose}>
            <div class="size-3 rounded-full bg-[#ff5f57]" />
        </button>
        <button on:click={handleMinimize}>
            <div class="size-3 rounded-full bg-[#febc2e]" />
        </button>
        <button on:click={handleFullscreen}>
            <div class="size-3 rounded-full bg-[#28c840]" />
        </button>
    </div>
    <div class="flex h-full items-center gap-1.5">
        <Button
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
        <div class="flex size-full items-center gap-1 px-3">
            <ZakuVector size={11} />
            <span class="pt-[5px] font-mono text-micro">v{version}</span>
        </div>
    </div>
</div>
