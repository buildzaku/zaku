<script lang="ts">
    import type { PaneAPI } from "paneforge";
    import { ChevronDownIcon, ChevronUpIcon } from "lucide-svelte";

    import { KeyValueList } from "$lib/components/key-value-list";
    import { Tabs, TabsList, TabsTrigger, TabsContent } from "$lib/components/primitives/tabs";
    import type { KeyValuePair } from "$lib/utils/api";
    import { REQUEST_BODY_TYPES } from "$lib/utils/constants";
    import { RequestBody } from "$lib/components/request-config-pane";
    import { Button } from "$lib/components/primitives/button";
    import { cn } from "$lib/utils/style";

    export let pane: PaneAPI;
    export let isCollapsed: boolean;
    export let parameters: KeyValuePair[];
    export let headers: KeyValuePair[];

    let body = REQUEST_BODY_TYPES.None;
</script>

<div class="size-full bg-card">
    <Tabs value="parameters">
        <div
            class={cn(
                "flex h-8 w-full items-center justify-between border-y bg-accent/25",
                isCollapsed ? "border-b-transparent" : "",
            )}
        >
            <div class="px-1.5">
                <TabsList
                    class="grid auto-cols-min grid-flow-col justify-start gap-2 p-0 [&>*]:text-xs"
                >
                    <TabsTrigger value="parameters">Parameters</TabsTrigger>
                    <TabsTrigger value="headers">Headers</TabsTrigger>
                    <TabsTrigger value="body">Body</TabsTrigger>
                </TabsList>
            </div>
            <Button
                variant="ghost"
                on:click={() => {
                    if (isCollapsed) {
                        pane.expand();
                        pane.resize(40);
                    } else {
                        pane.collapse();
                    }
                }}
            >
                {#if isCollapsed}
                    <ChevronUpIcon size={14} />
                {:else}
                    <ChevronDownIcon size={14} />
                {/if}
            </Button>
        </div>

        <div>
            <TabsContent value="parameters" class="m-0 p-3">
                <div class="mb-3">Query Parameters</div>
                <KeyValueList type="parameter" bind:pairs={parameters} />
            </TabsContent>
            <TabsContent value="headers" class="m-0 p-3">
                <div class="mb-3">Headers</div>
                <KeyValueList type="header" bind:pairs={headers} />
            </TabsContent>
            <TabsContent value="body" class="m-0">
                <RequestBody bind:selected={body} />
            </TabsContent>
        </div>
    </Tabs>
</div>
