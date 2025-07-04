<script lang="ts">
    import { onDestroy, onMount, tick } from "svelte";
    import { listen, TauriEvent } from "@tauri-apps/api/event";
    import type { UnlistenFn } from "@tauri-apps/api/event";

    import { TreeItemType } from "$lib/models";
    import { zakuState, treeActionsState, treeItemsState } from "$lib/state.svelte";
    import { cn, getMethodColorClass } from "$lib/utils/style";
    import { CollectionIcon } from "$lib/components/icons";
    import { commands } from "$lib/bindings";

    type Props = {
        parentRelativePath: string;
        type: TreeItemType;
        level: number;
        class?: string;
    };

    let { parentRelativePath, type, level, class: className }: Props = $props();

    let inputName: string = $state("");
    let inputElement: HTMLElement | null = $state(null);
    let unlistenWindowBlurEvent: UnlistenFn | null = $state(null);

    function initialize(element: HTMLElement) {
        element.focus();
    }

    function isRelatedElementExcludedFromFocusOutTarget(event: FocusEvent) {
        if (event.relatedTarget && event.relatedTarget instanceof HTMLElement) {
            return (
                event.relatedTarget.hasAttribute("data-create-tree-item-input") ||
                event.relatedTarget.hasAttribute("data-create-tree-item-button")
            );
        }

        return false;
    }

    async function handleCreateRequestOrCollection() {
        if (type === "collection") {
            const createCollectionResult = await commands.createCollection({
                parent_relative_path: parentRelativePath,
                relative_path: inputName,
            });

            if (createCollectionResult.status === "error") {
                console.error(createCollectionResult.error);

                return;
            }

            inputName = "";
            await zakuState.synchronize();

            treeItemsState.focussedItem = {
                type: TreeItemType.Collection,
                parentRelativePath: createCollectionResult.data.parent_relative_path,
                relativePath: createCollectionResult.data.relative_path,
            };

            const createdCollection = document.querySelector(
                `[data-current-path="${createCollectionResult.data.relative_path}"]`,
            );
            if (createdCollection) {
                createdCollection.scrollIntoView({ behavior: "instant", block: "center" });
            }
        } else {
            const createRequestResult = await commands.createRequest({
                parent_relative_path: parentRelativePath,
                relative_path: inputName,
            });

            if (createRequestResult.status === "error") {
                console.error(createRequestResult.error);

                return;
            }

            inputName = "";
            await zakuState.synchronize();

            treeItemsState.focussedItem = {
                type: TreeItemType.Request,
                parentRelativePath: createRequestResult.data.parent_relative_path,
                relativePath: createRequestResult.data.relative_path,
            };

            const createdRequest = document.querySelector(
                `[data-current-path="${createRequestResult.data.relative_path}"]`,
            );
            if (createdRequest) {
                createdRequest.scrollIntoView({ behavior: "instant", block: "center" });
            }
        }
    }

    onMount(async () => {
        unlistenWindowBlurEvent = await listen(TauriEvent.WINDOW_BLUR, () => {
            if (inputElement) {
                inputElement.blur();
            }
        });
    });

    onDestroy(() => {
        if (unlistenWindowBlurEvent) {
            unlistenWindowBlurEvent();
        }
    });
</script>

<div class={cn("relative min-w-full", className)}>
    {#if level > 1}
        <div
            style="left: {level * 8 + 3.5}px;"
            class="group-hover/explorer:bg-border/80 pointer-events-none absolute z-10 h-full w-px bg-transparent"
        ></div>
    {/if}
    <div
        style="padding-left: {level * 8}px"
        class="hover:bg-accent/60 focus-visible:ring-ring flex h-[22px] w-full items-center gap-2 overflow-hidden text-ellipsis whitespace-nowrap ring-inset focus-visible:ring-1 focus-visible:outline-none"
    >
        <div class="flex h-full w-full items-center gap-1 pl-1.5">
            {#if type === "collection"}
                <div class="w-[12px] min-w-[12px]"></div>
                <CollectionIcon size={12} class="min-h-[12px] min-w-[12px]" />
            {:else}
                <span class={cn("pl-3 text-[9px] font-bold", getMethodColorClass("GET"))}>GET</span>
            {/if}
            <input
                use:initialize
                bind:this={inputElement}
                data-create-tree-item-input
                type="text"
                onfocusout={async event => {
                    if (!isRelatedElementExcludedFromFocusOutTarget(event)) {
                        treeActionsState.createNewItem = null;
                        inputName = "";
                    } else {
                        inputName = "";
                        await tick();

                        if (inputElement) {
                            inputElement.focus();
                        }
                    }
                }}
                onkeydown={async keyboardEvent => {
                    if (keyboardEvent.key === "Enter") {
                        keyboardEvent.preventDefault();

                        await handleCreateRequestOrCollection();
                    }
                }}
                class="hover:bg-accent/60 focus-visible:ring-ring w-full text-sm whitespace-nowrap ring-inset focus-visible:ring-1 focus-visible:outline-none"
                bind:value={inputName}
            />
        </div>
    </div>
</div>
