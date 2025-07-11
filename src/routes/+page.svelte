<script lang="ts">
    import { Button } from "$lib/components/primitives/button";
    import { goto } from "$app/navigation";
    import { sharedState } from "$lib/state.svelte";
    import { SpaceCreateDialog } from "$lib/components/space";
    import { commands } from "$lib/bindings";

    let isCreateSpaceDialogOpen = $state(false);

    async function handleOpenExistingSpace() {
        try {
            const cmdResult = await commands.openDirDialog({ title: "Open an existing Space" });
            if (cmdResult.status === "error") {
                throw new Error("Unable to open existing space");
            }
            if (!cmdResult.data) {
                return;
            }

            const spaceRefCmdResult = await commands.getSpaceref(cmdResult.data);
            if (spaceRefCmdResult.status === "error") {
                throw new Error(`Cannot get space reference for ${cmdResult.data}`);
            }

            await sharedState.setActiveSpace(spaceRefCmdResult.data);
            await goto("/space");
        } catch (err) {
            console.error(err);
            await commands.dispatchNotif({
                title: "Doesn't look like a valid space.",
                body: "Unable to parse the directory, make sure it is a valid space and try again.",
            });
        }
    }
</script>

<div class="flex size-full flex-col items-center justify-center gap-2">
    <h1 class="my-2 text-2xl font-medium">Welcome to Zaku</h1>
    <Button
        variant="outline"
        onclick={() => {
            isCreateSpaceDialogOpen = true;
        }}
    >
        + Create Space
    </Button>
    <Button
        variant="link"
        class="text-foreground hover:no-underline"
        onclick={handleOpenExistingSpace}
    >
        + Open Existing Space
    </Button>
</div>

<SpaceCreateDialog
    bind:isOpen={isCreateSpaceDialogOpen}
    onCreate={async () => {
        await goto("/space");
    }}
/>
