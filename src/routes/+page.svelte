<script>
    import { Button } from "$lib/components/ui/button";
    import { toggleMode } from "mode-watcher";
</script>

<div class="w-dvw h-dvh flex flex-col gap-4 justify-center items-center">
    <h1 class="text-4xl font-semibold">Zaku</h1>
    <Button on:click={toggleMode}>Hello!</Button>
</div>
