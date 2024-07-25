<script lang="ts">
    import { ModeWatcher } from "mode-watcher";
    import "../app.css";
</script>

<ModeWatcher defaultMode="dark" track={false} />
<slot />
