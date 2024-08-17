<script lang="ts">
    import { Sun, Moon } from "svelte-radix";
    import { toggleMode } from "mode-watcher";

    import { cn } from "$lib/utils/style";
    import { ZakuVector } from "$lib/components/vectors";
    import { Button } from "$lib/components/primitives/button";
</script>

<div
    data-tauri-drag-region
    class={cn("flex w-full cursor-default items-center justify-between", $$props["class"])}
>
    <div />
    <div class="flex h-full items-center">
        <Button
            on:click={toggleMode}
            variant="ghost-hover"
            size="icon"
            class="size-6 min-h-6 min-w-6 p-1"
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
