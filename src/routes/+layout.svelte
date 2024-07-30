<script lang="ts">
    import { ModeWatcher } from "mode-watcher";

    import "../app.css";
    import { TitleBar } from "$lib/components/title-bar";
</script>

<ModeWatcher defaultMode="dark" track={false} />
<main class="h-dvh w-dvw rounded-lg border-[1px] bg-background">
    <TitleBar class="h-8" />
    <div class="h-[calc(100dvh-2rem-1px)]">
        <slot />
    </div>
</main>
