<script lang="ts">
    import { ModeWatcher } from "mode-watcher";

    import "../app.css";
    import { TitleBar } from "$lib/components/title-bar";
</script>

<ModeWatcher defaultMode="dark" track={false} />
<TitleBar />
<slot />
