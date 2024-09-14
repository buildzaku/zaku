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
    import { createSpace } from "$lib/store";
    import { dispatchNotification, openDirectoryDialog } from "$lib/commands";

    type $$Props = {
        isOpen: boolean;
        onCreate?: () => Promise<void>;
    };

    export let onCreate: $$Props["onCreate"] = undefined;
    export let isOpen: $$Props["isOpen"] = false;

    let createSpaceName: string = "";
    let createSpaceLocation: string = "";

    async function handleCreateSpaceBrowse() {
        const selected = await openDirectoryDialog({ title: "Create a new Space" });

        if (selected !== null) {
            createSpaceLocation = selected;

            const spacePathContainerElement = document.getElementById("space-path-container");

            if (spacePathContainerElement) {
                await tick();
                const rightMostPosition = spacePathContainerElement.scrollWidth;
                spacePathContainerElement.scrollLeft = rightMostPosition;
            }
        }
    }

    async function handleCreateSpace() {
        try {
            await createSpace({ name: createSpaceName, location: createSpaceLocation });

            if (onCreate) {
                await onCreate();
            }
        } catch (err) {
            console.error(err);
            await dispatchNotification({
                title: "Something went wrong.",
                body: `Unable to create space "${createSpaceName}", make sure the directory exists and try again.`,
            });
        }
    }
</script>

<Dialog
    open={isOpen}
    onOpenChange={() => {
        createSpaceName = "";
        createSpaceLocation = "";
    }}
    onOutsideClick={() => {
        isOpen = false;
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
                        class="scrollbar-hidden flex h-6 w-full select-text items-center overflow-y-hidden overflow-x-scroll whitespace-nowrap text-nowrap rounded-md rounded-r-none border border-r-0 border-input bg-transparent px-3 py-1 text-small shadow-sm"
                        on:click={handleCreateSpaceBrowse}
                    >
                        {createSpaceLocation}
                    </button>
                    <Button
                        on:click={handleCreateSpaceBrowse}
                        class="col-span-1 h-6 w-[80px] rounded-l-none"
                        variant="outline"
                    >
                        Browse
                    </Button>
                </div>
            </div>
        </div>
        <DialogFooter>
            <Button type="submit" on:click={handleCreateSpace}>Create</Button>
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
