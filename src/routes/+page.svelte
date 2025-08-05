<script lang="ts">
  import { Button } from "$lib/components/primitives/button";
  import { goto } from "$app/navigation";
  import { appStateRx } from "$lib/state.svelte";
  import { SpaceCreateDialog } from "$lib/components/space";
  import { commands } from "$lib/bindings";
  import { emitCmdError } from "$lib/utils";

  let isCreateSpaceDialogOpen = $state(false);

  async function handleOpenExistingSpace() {
    const openDirDialogResult = await commands.openDirDialog({
      title: "Open an existing Space",
    });
    if (openDirDialogResult.status !== "ok") {
      return emitCmdError(openDirDialogResult.error);
    }
    if (!openDirDialogResult.data) {
      return;
    }

    const getSpaceRefResult = await commands.getSpaceref(openDirDialogResult.data);
    if (getSpaceRefResult.status !== "ok") {
      return emitCmdError(getSpaceRefResult.error);
    }

    await appStateRx.setSpace(getSpaceRefResult.data);
    await goto("/space");
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
