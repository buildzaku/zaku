<script lang="ts">
    import { onDestroy, onMount, tick } from "svelte";
    import { listen, TauriEvent } from "@tauri-apps/api/event";
    import type { UnlistenFn } from "@tauri-apps/api/event";

    import { TREE_ITEM_TYPE } from "$lib/models";
    import { createNewTreeItem, focussedTreeItem, zakuState } from "$lib/store";
    import { cn, getMethodColorClass } from "$lib/utils/style";
    import { CollectionIcon } from "$lib/components/icons";
    import type { ValueOf } from "$lib/utils";
    import { safeInvoke } from "$lib/commands";
    import type {
        CreateCollectionDto,
        CreateNewCollectionOrRequest,
        CreateRequestDto,
    } from "$lib/bindings";

    export let parentRelativePath: string;
    export let type: ValueOf<typeof TREE_ITEM_TYPE>;
    export let level: number;

    let propsClass = $$props["class"];
    let inputName: string;
    let inputElement: HTMLElement | null = null;
    let unlistenWindowBlurEvent: UnlistenFn | null = null;

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
        if (type === TREE_ITEM_TYPE.Collection) {
            const create_collection_dto: CreateCollectionDto = {
                parent_relative_path: parentRelativePath,
                relative_path: inputName,
            };
            const createCollectionResult = await safeInvoke<CreateNewCollectionOrRequest>(
                "create_collection",
                { create_collection_dto },
            );

            if (!createCollectionResult.ok) {
                console.error(createCollectionResult.err);

                return;
            }

            inputName = "";
            await zakuState.synchronize();

            focussedTreeItem.set({
                type: TREE_ITEM_TYPE.Collection,
                parentRelativePath: createCollectionResult.value.parent_relative_path,
                relativePath: createCollectionResult.value.relative_path,
            });

            const createdCollection = document.querySelector(
                `[data-current-path="${createCollectionResult.value.relative_path}"]`,
            );
            if (createdCollection) {
                createdCollection.scrollIntoView({ behavior: "instant", block: "center" });
            }
        } else {
            const create_request_dto: CreateRequestDto = {
                parent_relative_path: parentRelativePath,
                relative_path: inputName,
            };
            const createRequestResult = await safeInvoke<CreateNewCollectionOrRequest>(
                "create_request",
                { create_request_dto },
            );

            if (!createRequestResult.ok) {
                console.error(createRequestResult.err);

                return;
            }

            inputName = "";
            await zakuState.synchronize();

            focussedTreeItem.set({
                type: TREE_ITEM_TYPE.Request,
                parentRelativePath: createRequestResult.value.parent_relative_path,
                relativePath: createRequestResult.value.relative_path,
            });

            const createdRequest = document.querySelector(
                `[data-current-path="${createRequestResult.value.relative_path}"]`,
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

<div class={cn("relative min-w-full", propsClass)}>
    {#if level > 1}
        <div
            style="left: {level * 8 + 3.5}px;"
            class="pointer-events-none absolute z-10 h-full w-px bg-transparent group-hover/explorer:bg-border/80"
        ></div>
    {/if}
    <div
        style="padding-left: {level * 8}px"
        class="flex h-[22px] w-full items-center gap-2 overflow-hidden text-ellipsis whitespace-nowrap ring-inset hover:bg-accent/60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
    >
        <div class="flex h-full w-full items-center gap-1 pl-1.5">
            {#if type === TREE_ITEM_TYPE.Collection}
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
                on:focusout={async event => {
                    if (!isRelatedElementExcludedFromFocusOutTarget(event)) {
                        createNewTreeItem.set(null);
                        inputName = "";
                    } else {
                        inputName = "";
                        await tick();

                        if (inputElement) {
                            inputElement.focus();
                        }
                    }
                }}
                on:keydown={async keyboardEvent => {
                    if (keyboardEvent.key === "Enter") {
                        keyboardEvent.preventDefault();

                        await handleCreateRequestOrCollection();
                    }
                }}
                class="w-full whitespace-nowrap text-sm ring-inset hover:bg-accent/60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                bind:value={inputName}
            />
        </div>
    </div>
</div>
