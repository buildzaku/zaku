<script lang="ts">
    import { Button, buttonVariants } from "$lib/components/primitives/button";
    import {
        Dialog,
        DialogContent,
        DialogDescription,
        DialogFooter,
        DialogHeader,
        DialogTitle,
        DialogTrigger,
    } from "$lib/components/primitives/dialog";
    import { Label } from "$lib/components/primitives/label";
    import { Input } from "$lib/components/primitives/input";
    import { tick } from "svelte";
    import { Struct } from "$lib/utils/struct";
    import { goto } from "$app/navigation";
    import { activeSpace, createSpace } from "$lib/store";
    import { dispatchNotification, openDirectoryDialog } from "$lib/commands";

    let createSpaceName: string = "";
    let createSpacePath: string = "";

    let openExistingSpacePath: string = "";

    async function handleCreateSpaceBrowse() {
        const selected = await openDirectoryDialog({ title: "Create a new Space" });

        if (selected !== null) {
            createSpacePath = selected;

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
            const spaceSchema = Struct.strictObject({
                name: Struct.pipe(Struct.string(), Struct.minLength(1)),
                path: Struct.pipe(Struct.string(), Struct.minLength(1)),
            });

            const spaceData = Struct.parse(spaceSchema, {
                name: createSpaceName,
                path: createSpacePath,
            });

            await createSpace(spaceData);
            await goto("/space");
        } catch (err) {
            console.error(err);
            await dispatchNotification({
                title: "Something went wrong.",
                body: `Unable to create space "${createSpaceName}", make sure the directory exists and try again.`,
            });
        }
    }

    async function handleOpenExistingSpace() {
        try {
            const selectedPath = await openDirectoryDialog({ title: "Open an existing Space" });

            if (selectedPath !== null) {
                openExistingSpacePath = selectedPath;
                await activeSpace.set(selectedPath);
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
    <Dialog
        onOpenChange={() => {
            createSpaceName = "";
            createSpacePath = "";
        }}
    >
        <DialogTrigger class={buttonVariants({ variant: "outline" })}>+ Create Space</DialogTrigger>
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
                            {createSpacePath}
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
    <Button
        variant="link"
        class="text-foreground hover:no-underline"
        on:click={handleOpenExistingSpace}
    >
        + Open Existing Space
    </Button>
</div>

<style lang="postcss">
    .scrollbar-hidden {
        -ms-overflow-style: none;
        scrollbar-width: none;
    }
    .scrollbar-hidden::-webkit-scrollbar {
        display: none;
    }
</style>
