<script lang="ts">
    import { SunIcon, MoonIcon } from "@lucide/svelte";
    import { toggleMode } from "mode-watcher";

    import { cn } from "$lib/utils/style";
    import { ZakuIcon } from "$lib/components/icons";
    import { Button } from "$lib/components/primitives/button";
</script>

<div
    data-tauri-drag-region
    class={cn("flex w-full cursor-default items-center justify-between", $$props["class"])}
>
    <div></div>
    <div class="flex h-full items-center gap-3 px-4">
        <Button onclick={toggleMode} variant="ghost" size="icon" class="size-6 min-h-6 min-w-6 p-1">
            <SunIcon size={14} class="block dark:hidden" aria-label="light" />
            <MoonIcon size={14} class="hidden dark:block" aria-label="dark" />
            <span class="sr-only">Toggle theme</span>
        </Button>
        <div>
            <ZakuIcon size={10} />
        </div>
    </div>
</div>
