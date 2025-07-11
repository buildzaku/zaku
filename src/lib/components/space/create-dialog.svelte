<script lang="ts">
    import { Button } from "$lib/components/primitives/button";
    import {
        Dialog,
        DialogContent,
        DialogDescription,
        DialogFooter,
        DialogHeader,
        DialogTitle,
    } from "$lib/components/primitives/dialog";
    import { Label } from "$lib/components/primitives/label";
    import { Input } from "$lib/components/primitives/input";
    import { tick } from "svelte";
    import { sharedState } from "$lib/state.svelte";
    import { commands } from "$lib/bindings";

    type Props = {
        isOpen: boolean;
        onCreate?: () => Promise<void>;
    };

    let { isOpen = $bindable(), onCreate = async () => {} }: Props = $props();

    let createSpaceName: string = $state("");
    let createSpaceLocation: string = $state("");

    async function handleCreateSpaceBrowse() {
        const cmdResult = await commands.openDirDialog({ title: "Create a new Space" });
        if (cmdResult.status === "error") {
            throw new Error("Unable to open selected directory");
        }
        if (!cmdResult.data) {
            return;
        }

        createSpaceLocation = cmdResult.data;

        const spacePathContainerElement = document.getElementById("space-path-container");
        if (spacePathContainerElement) {
            await tick();
            const rightMostPosition = spacePathContainerElement.scrollWidth;
            spacePathContainerElement.scrollLeft = rightMostPosition;
        }
    }

    async function handleCreateSpace() {
        const spaceReference = await commands.createSpace({
            name: createSpaceName,
            location: createSpaceLocation,
        });

        if (spaceReference.status === "error") {
            await commands.dispatchNotif({
                title: "Something went wrong.",
                body: `Unable to create space "${createSpaceName}", make sure the directory exists and try again.`,
            });

            return;
        }

        await sharedState.setActiveSpace(spaceReference.data);
        isOpen = false;

        await onCreate();
    }
</script>

<Dialog
    bind:open={isOpen}
    onOpenChange={() => {
        createSpaceName = "";
        createSpaceLocation = "";
    }}
>
    <DialogContent class="w-[424px] max-w-[424px]">
        <DialogHeader>
            <DialogTitle>Create a new Space</DialogTitle>
            <DialogDescription>
                Separate your projects, work and more. Choose a name and where to save it.
            </DialogDescription>
        </DialogHeader>
        <div class="flex w-full flex-col gap-4 py-4">
            <div class="flex flex-col gap-1">
                <Label for="name">Name</Label>
                <Input id="name" bind:value={createSpaceName} />
            </div>
            <div class="flex max-w-[374px] flex-col gap-1">
                <Label for="location">Location</Label>
                <div class="flex h-6 w-full">
                    <button
                        id="space-path-container"
                        class="scrollbar-hidden border-input text-small flex h-6 w-full items-center overflow-x-scroll overflow-y-hidden rounded-md rounded-r-none border border-r-0 bg-transparent px-3 py-1 text-nowrap whitespace-nowrap shadow-sm select-text"
                        onclick={handleCreateSpaceBrowse}
                    >
                        {createSpaceLocation}
                    </button>
                    <Button
                        onclick={handleCreateSpaceBrowse}
                        class="col-span-1 h-6 w-[80px] rounded-l-none"
                        variant="outline"
                    >
                        Browse
                    </Button>
                </div>
            </div>
        </div>
        <DialogFooter>
            <Button type="submit" onclick={handleCreateSpace}>Create</Button>
        </DialogFooter>
    </DialogContent>
</Dialog>

<style lang="postcss">
    .scrollbar-hidden {
        -ms-overflow-style: none;
        scrollbar-width: none;
    }
    .scrollbar-hidden::-webkit-scrollbar {
        display: none;
    }
</style>
