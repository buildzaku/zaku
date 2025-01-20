<script lang="ts">
    import { Button } from "$lib/components/primitives/button";
    import { goto } from "$app/navigation";
    import { zakuState } from "$lib/state.svelte";
    import { dispatchNotification, getSpaceReference, openDirectoryDialog } from "$lib/commands";
    import { SpaceCreateDialog } from "$lib/components/space";

    let isCreateSpaceDialogOpen = $state(false);

    async function handleOpenExistingSpace() {
        try {
            const selectedPath = await openDirectoryDialog({ title: "Open an existing Space" });

            if (selectedPath !== null) {
                const spaceReference = await getSpaceReference(selectedPath);

                await zakuState.setActiveSpace(spaceReference);
                await goto("/space");
            }
        } catch (err) {
            console.error(err);
            await dispatchNotification({
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
