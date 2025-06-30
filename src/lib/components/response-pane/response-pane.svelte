<script lang="ts">
    import { RefreshCwIcon, RocketIcon } from "lucide-svelte";
    import type { PaneAPI } from "paneforge";
    import { ChevronDownIcon, ChevronUpIcon } from "lucide-svelte";

    import { Tabs, TabsList, TabsTrigger, TabsContent } from "$lib/components/primitives/tabs";
    import type { RequestStatus } from "$lib/utils/api";
    import { Alert } from "$lib/components/primitives/alert";
    import { CodeBlock } from "$lib/components/code-block";
    import { Button } from "$lib/components/primitives/button";
    import { json } from "@codemirror/lang-json";
    import { Badge } from "../primitives/badge";
    import { HTTP_STATUS_DESCRIPTION } from "$lib/utils/constants";
    import { treeItemsState } from "$lib/state.svelte";

    type Props = {
        pane: PaneAPI;
        isCollapsed: boolean;
        requestStatus: RequestStatus;
        preview: string;
        error: string;
    };

    let {
        pane,
        isCollapsed = $bindable(),
        requestStatus = $bindable(),
        preview = $bindable(),
        error = $bindable(),
    }: Props = $props();

    function prettyJson(data: string | undefined) {
        if (!data) return String();

        try {
            return JSON.stringify(JSON.parse(data), null, 2);
        } catch {
            return data;
        }
    }

    let jsonPrettyResponse = $derived(
        treeItemsState.activeRequest && treeItemsState.activeRequest.self.response
            ? prettyJson(treeItemsState.activeRequest.self.response.data)
            : "",
    );
    let plainTextResponse = $derived(treeItemsState.activeRequest?.self.response?.data);
</script>

{#snippet statusBadge(status: number)}
    <div class="mr-3 flex items-center font-mono">
        // TODO - FIX
        {#if status === 200}
            <Badge variant="success">
                {status}
                {HTTP_STATUS_DESCRIPTION[status] ?? "Unknown"}
            </Badge>
        {:else}
            <Badge variant="failure">
                {status}
                {HTTP_STATUS_DESCRIPTION[status] ?? "Unknown"}
            </Badge>
        {/if}
    </div>
{/snippet}

{#if treeItemsState.activeRequest}
    <div class="bg-card size-full">
        {#if requestStatus === "idle"}
            {#if isCollapsed}
                <div class="bg-accent/25 flex h-8 w-full items-center justify-between border-b">
                    <div class="flex size-full items-center justify-end">
                        <Button
                            variant="ghost"
                            onclick={() => {
                                pane.expand();
                                pane.resize(60);
                            }}
                            class="hover:bg-transparent"
                        >
                            <span class="pr-1.5 text-xs font-medium">Response</span>
                            <ChevronUpIcon size={14} />
                        </Button>
                    </div>
                </div>
            {:else}
                <div class="bg-accent/25 flex h-8 w-full items-center justify-between border-b">
                    <div class="flex size-full items-center justify-end">
                        <Button
                            variant="ghost"
                            onclick={() => {
                                pane.collapse();
                            }}
                            class="hover:bg-transparent"
                        >
                            <span class="pr-1.5 text-xs font-medium">Response</span>
                            <ChevronDownIcon size={14} />
                        </Button>
                    </div>
                </div>
                <div class="flex size-full items-center justify-center gap-2 pb-8">
                    <RocketIcon size="20" />
                    <span>
                        Hit <b class="font-semibold">Send</b> to make a request
                    </span>
                </div>
            {/if}
        {:else if requestStatus === "loading"}
            <div class="flex size-full items-center justify-center">
                <RefreshCwIcon
                    strokeWidth={1.5}
                    absoluteStrokeWidth
                    size={20}
                    class="mr-3 animate-spin"
                />
            </div>
        {:else}
            <Tabs value="body" class="size-full">
                <div
                    class="bg-accent/25 flex h-8 w-full items-center justify-between border-y border-t-transparent"
                >
                    {#if isCollapsed}
                        <button
                            class="flex h-8 w-full cursor-pointer items-center justify-end gap-1.5 border-b px-3"
                            onclick={() => {
                                pane.expand();
                                pane.resize(60);
                            }}
                        >
                            {#if treeItemsState.activeRequest.self.response}
                                {@render statusBadge(
                                    treeItemsState.activeRequest.self.response.status,
                                )}
                            {/if}
                            <span class="pr-1.5 text-xs font-medium">Response</span>
                            <ChevronUpIcon size={14} />
                        </button>
                    {:else}
                        <div class="px-1.5">
                            <TabsList
                                class="grid auto-cols-min grid-flow-col justify-start gap-2 p-0 [&>*]:text-xs"
                            >
                                <TabsTrigger value="body">Body</TabsTrigger>
                                <TabsTrigger value="cookies">Cookies</TabsTrigger>
                                <TabsTrigger value="headers">Headers</TabsTrigger>
                            </TabsList>
                        </div>
                        <div class="flex h-8 w-full items-center justify-end gap-1.5 border-b px-3">
                            {#if treeItemsState.activeRequest.self.response}
                                {@render statusBadge(
                                    treeItemsState.activeRequest.self.response.status,
                                )}
                            {/if}
                            <button
                                onclick={() => {
                                    pane.collapse();
                                }}
                                class="flex cursor-pointer items-center gap-1.5 hover:bg-transparent"
                            >
                                <span class="pr-1.5 text-xs font-medium">Response</span>
                                <ChevronDownIcon size={14} />
                            </button>
                        </div>
                    {/if}
                </div>
                {#if !isCollapsed}
                    <div class="flex h-[calc(100%-2.25rem)] w-full items-center justify-center">
                        <TabsContent value="body" class="m-0 size-full">
                            {#if requestStatus === "success"}
                                <Tabs value="pretty" class="size-full">
                                    <div class="flex items-center justify-end border-b px-3">
                                        <TabsList
                                            class="my-1 auto-cols-min grid-flow-col gap-2 p-0"
                                        >
                                            <TabsTrigger value="pretty">Pretty</TabsTrigger>
                                            <TabsTrigger value="raw">Raw</TabsTrigger>
                                            <TabsTrigger value="preview">Preview</TabsTrigger>
                                        </TabsList>
                                    </div>
                                    <div
                                        class="h-[calc(100%-2.25rem)] w-full overflow-scroll [&>*]:m-0"
                                    >
                                        <TabsContent value="pretty" class="size-full">
                                            <CodeBlock
                                                language={json()}
                                                readOnly={true}
                                                bind:value={jsonPrettyResponse}
                                                class="size-full"
                                            />
                                        </TabsContent>
                                        <TabsContent value="raw" class="size-full">
                                            <CodeBlock
                                                language={null}
                                                readOnly={true}
                                                bind:value={plainTextResponse}
                                                class="size-full"
                                            />
                                        </TabsContent>
                                        <TabsContent value="preview" class="size-full">
                                            <iframe
                                                title=""
                                                src="about:blank"
                                                srcdoc={preview}
                                                class="size-full"
                                                loading="lazy"
                                                sandbox=""
                                            ></iframe>
                                        </TabsContent>
                                    </div>
                                </Tabs>
                            {:else if requestStatus === "error"}
                                <div class="flex size-full items-center justify-center gap-2">
                                    <Alert
                                        variant="destructive"
                                        class="w-fit max-w-[50%] py-1 [&>*]:select-text"
                                    >
                                        <span class="font-semibold">Error: </span>
                                        <span>{error}</span>
                                    </Alert>
                                </div>
                            {/if}
                        </TabsContent>
                        <TabsContent value="cookies">WIP</TabsContent>
                        <TabsContent value="headers">WIP</TabsContent>
                    </div>
                {/if}
            </Tabs>
        {/if}
    </div>
{/if}
