<script lang="ts">
    import { onDestroy, onMount, tick } from "svelte";
    import { listen, TauriEvent } from "@tauri-apps/api/event";
    import type { UnlistenFn } from "@tauri-apps/api/event";

    import { sharedState, explorerActionsState, explorerState } from "$lib/state.svelte";
    import { cn, requestColors } from "$lib/utils/style";
    import { CollectionIcon } from "$lib/components/icons";
    import { commands } from "$lib/bindings";
    import { toast } from "svelte-sonner";
    import { emitCmdError } from "$lib/utils";

    type Props = {
        parentRelativePath: string;
        type: "collection" | "request";
        level: number;
        class?: string;
    };

    let { parentRelativePath, type, level, class: className }: Props = $props();

    let inputRelpath: string = $state("");
    let inputElement: HTMLElement | null = $state(null);
    let unlistenWindowBlurEvent: UnlistenFn | null = $state(null);

    function initialize(element: HTMLElement) {
        element.focus();
    }

    function isRelatedElementExcludedFromFocusOutTarget(event: FocusEvent) {
        if (event.relatedTarget && event.relatedTarget instanceof HTMLElement) {
            return (
                event.relatedTarget.hasAttribute("data-create-tree-node-input") ||
                event.relatedTarget.hasAttribute("data-create-tree-node-button")
            );
        }

        return false;
    }

    async function handleCreateRequestOrCollection() {
        if (type === "collection") {
            const createCollectionResult = await commands.createCollection({
                location_relpath: parentRelativePath,
                relpath: inputRelpath,
            });
            if (createCollectionResult.status !== "ok") {
                return emitCmdError(createCollectionResult.error);
            }

            inputRelpath = "";
            await sharedState.synchronize();

            explorerState.focussedNode = {
                type: "collection",
                parentRelativePath: createCollectionResult.data.parent_relpath,
                relativePath: createCollectionResult.data.relpath,
            };

            const createdCollection = document.querySelector(
                `[data-current-path="${createCollectionResult.data.relpath}"]`,
            );
            if (createdCollection) {
                createdCollection.scrollIntoView({ behavior: "instant", block: "center" });
            }
        } else {
            const createReqResult = await commands.createReq({
                location_relpath: parentRelativePath,
                relpath: inputRelpath,
            });

            if (createReqResult.status === "error") {
                // TODO - get error from cmd
                toast.error(`Unable to create request`);
                console.error(createReqResult.error);

                return;
            }

            inputRelpath = "";
            await sharedState.synchronize();

            explorerState.focussedNode = {
                type: "request",
                parentRelativePath: createReqResult.data.parent_relpath,
                relativePath: createReqResult.data.relpath,
            };

            const createdRequest = document.querySelector(
                `[data-current-path="${createReqResult.data.relpath}"]`,
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
                <span class="pl-3 text-[9px] font-bold {requestColors({ method: 'GET' })}">
                    GET
                </span>
            {/if}
            <input
                use:initialize
                bind:this={inputElement}
                data-create-tree-node-input
                type="text"
                onfocusout={async event => {
                    if (!isRelatedElementExcludedFromFocusOutTarget(event)) {
                        explorerActionsState.createNewNode = null;
                        inputRelpath = "";
                    } else {
                        inputRelpath = "";
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
                bind:value={inputRelpath}
            />
        </div>
    </div>
</div>
