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
    import { open } from "@tauri-apps/plugin-dialog";
    import { readDir } from "@tauri-apps/plugin-fs";
    import { tick } from "svelte";
    import { Struct } from "$lib/utils/struct";
    import { goto } from "$app/navigation";
    import { activeWorkspace, createWorkspace } from "$lib/store";

    let workspaceName: string = "";
    let workspacePath: string = "";

    async function handleBrowse() {
        // Open a selection dialog for directories
        const selected = await open({
            directory: true,
            multiple: false,
        });
        if (selected === null) {
            console.log(2);
            // user cancelled the selection
        } else {
            console.log(3);
            console.log("selected: ", selected);
            // workspacePath = selected.length > 35 ? String("...") + selected.slice(-35) : selected;
            workspacePath = selected;

            const scrollable = document.getElementById("workspace-path");
            console.log({ scrollable });
            if (scrollable) {
                await tick();
                scrollable.scrollLeft = scrollable.scrollWidth;
            }
            console.log("workspacePath: ", workspacePath);

            // user selected a single directory
        }
    }

    async function handleCreateWorkspace() {
        const workspaceSchema = Struct.strictObject({
            name: Struct.pipe(Struct.string(), Struct.minLength(1)),
            path: Struct.pipe(Struct.string(), Struct.minLength(1)),
        });

        console.log("yoyoyoyoyoy");
        console.log(workspaceName, workspacePath);

        const workspaceData = Struct.parse(workspaceSchema, {
            name: workspaceName,
            path: workspacePath,
        });

        console.log({ workspaceData });

        await createWorkspace(workspaceData);

        // await persistedStore.set(StoreKey.CurrentWorkspace, workspaceData);

        await goto("/workspace");
    }
</script>

<div class="flex size-full flex-col items-center justify-center gap-2">
    <h1 class="my-2 text-2xl font-medium">Welcome to Zaku</h1>
    <Dialog
        open={true}
        onOpenChange={() => {
            workspaceName = "";
            workspacePath = "";
        }}
    >
        <DialogTrigger class={buttonVariants({ variant: "outline" })}>
            + Create Workspace
        </DialogTrigger>
        <DialogContent class="w-[424px] max-w-[424px]">
            <DialogHeader>
                <DialogTitle>Create workspace</DialogTitle>
                <DialogDescription>
                    Make changes to your profile here. Click save when you're done.
                </DialogDescription>
            </DialogHeader>
            <div class="flex w-full flex-col gap-4 py-4">
                <div class="flex flex-col gap-1">
                    <Label for="name">Name</Label>
                    <Input id="name" bind:value={workspaceName} />
                </div>
                <div class="flex max-w-[374px] flex-col gap-1">
                    <Label for="location">Location</Label>
                    <div class="flex h-7 w-full">
                        <div
                            id="workspace-path"
                            class="container-peepoo flex h-7 w-full select-text items-center overflow-y-hidden overflow-x-scroll whitespace-nowrap text-nowrap rounded-md rounded-r-none border border-r-0 border-input bg-transparent px-3 py-1 text-small shadow-sm"
                        >
                            {workspacePath}
                        </div>
                        <Button
                            on:click={handleBrowse}
                            class="col-span-1 h-7 w-[120px] rounded-l-none"
                            variant="outline"
                        >
                            Browse
                        </Button>
                    </div>
                </div>
            </div>
            <DialogFooter>
                <Button type="submit" on:click={handleCreateWorkspace}>Create</Button>
            </DialogFooter>
        </DialogContent>
    </Dialog>
</div>

<style lang="postcss">
    .container-peepoo {
        -ms-overflow-style: none;
        scrollbar-width: none;
    }
    .container-peepoo::-webkit-scrollbar {
        display: none;
    }
</style>
