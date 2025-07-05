<script lang="ts">
    import { RefreshCwIcon, RocketIcon } from "@lucide/svelte";
    import type { PaneAPI } from "paneforge";
    import { ChevronDownIcon, ChevronUpIcon } from "@lucide/svelte";
    import { json } from "@codemirror/lang-json";

    import { Tabs, TabsList, TabsTrigger, TabsContent } from "$lib/components/primitives/tabs";
    import { Alert } from "$lib/components/primitives/alert";
    import { CodeBlock } from "$lib/components/code-block";
    import { Button } from "$lib/components/primitives/button";
    import { Badge } from "$lib/components/primitives/badge";
    import { HTTP_STATUS_DESCRIPTION } from "$lib/utils/constants";
    import type { ActiveRequest } from "$lib/models";

    type Props = {
        pane: PaneAPI;
        isCollapsed: boolean;
        activeReqRef: ActiveRequest;
    };

    let { pane, isCollapsed, activeReqRef }: Props = $props();

    function prettyJson(data: string | undefined) {
        if (!data) return String();

        try {
            return JSON.stringify(JSON.parse(data), null, 2);
        } catch {
            return data;
        }
    }
</script>

{#snippet statusBadge(status: number)}
    <div class="mr-3 flex items-center font-mono">
        {#if status >= 200 && status < 300}
            <Badge variant="success">
                {status}
                {HTTP_STATUS_DESCRIPTION[status] ?? ""}
            </Badge>
        {:else}
            <Badge variant="failure">
                {status}
                {HTTP_STATUS_DESCRIPTION[status] ?? ""}
            </Badge>
        {/if}
    </div>
{/snippet}

{#snippet keyValueTable(pairs: [string, string][])}
    <div class="m-3 h-[calc(100%-1.5rem)]">
        <div class="bg-card flex h-full flex-col overflow-hidden rounded border">
            <div class="bg-accent/25 flex border-b font-semibold">
                <div class="w-[35%] border-r p-2">Key</div>
                <div class="flex-1 p-2">Value</div>
            </div>
            <div class="flex-1 overflow-y-auto">
                {#each pairs as [key, value], idx (idx)}
                    <div class="flex border-b last:border-b-0">
                        <div class="w-[35%] border-r p-2 break-words">
                            {key}
                        </div>
                        <div class="flex-1 p-2 break-words">{value}</div>
                    </div>
                {/each}
            </div>
        </div>
    </div>
{/snippet}

<div class="size-full">
    {#if activeReqRef.self.status === "Idle"}
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
    {:else if activeReqRef.self.status === "Pending"}
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
                class="bg-card flex h-8 w-full items-center justify-between border-y border-t-transparent"
            >
                {#if isCollapsed}
                    <button
                        class="flex h-8 w-full cursor-pointer items-center justify-end gap-1.5 border-b px-3"
                        onclick={() => {
                            pane.expand();
                            pane.resize(60);
                        }}
                    >
                        {#if activeReqRef.self.response && activeReqRef.self.response.status}
                            {@render statusBadge(activeReqRef.self.response.status)}
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
                            <TabsTrigger value="cookies">
                                {"Cookies".concat(
                                    activeReqRef.self.response?.cookies
                                        ? ` (${activeReqRef.self.response?.cookies.length})`
                                        : "",
                                )}
                            </TabsTrigger>
                            <TabsTrigger value="headers">
                                {"Headers".concat(
                                    activeReqRef.self.response?.headers
                                        ? ` (${activeReqRef.self.response?.headers.length})`
                                        : "",
                                )}
                            </TabsTrigger>
                        </TabsList>
                    </div>
                    <div class="flex h-8 w-full items-center justify-end gap-1.5 border-b px-3">
                        {#if activeReqRef.self.response && activeReqRef.self.response.status}
                            {@render statusBadge(activeReqRef.self.response.status)}
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
                <div class="bg-background flex h-[calc(100%-32px)] w-full">
                    <TabsContent value="body" class="m-0 size-full">
                        {#if activeReqRef.self.status === "Success"}
                            <Tabs value="pretty" class="bg-card size-full">
                                <div class="flex items-center justify-end border-b px-3">
                                    <TabsList class="my-1 auto-cols-min grid-flow-col gap-2 p-0">
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
                                            value={prettyJson(activeReqRef.self.response?.data)}
                                            class="size-full"
                                        />
                                    </TabsContent>
                                    <TabsContent value="raw" class="size-full">
                                        <CodeBlock
                                            language={null}
                                            readOnly={true}
                                            value={activeReqRef.self.response?.data}
                                            class="size-full"
                                        />
                                    </TabsContent>
                                    <TabsContent value="preview" class="size-full">
                                        <iframe
                                            title=""
                                            src="about:blank"
                                            srcdoc={activeReqRef.self.response
                                                ? activeReqRef.self.response.data
                                                : ""}
                                            class="size-full"
                                            loading="lazy"
                                            sandbox=""
                                        ></iframe>
                                    </TabsContent>
                                </div>
                            </Tabs>
                        {:else if activeReqRef.self.status === "Error"}
                            {#if activeReqRef.self.response && activeReqRef.self.response.data}
                                <div class="flex size-full items-center justify-center gap-2">
                                    <Alert
                                        variant="destructive"
                                        class="w-fit max-w-[50%] py-1 [&>*]:select-text"
                                    >
                                        <span>{activeReqRef.self.response.data}</span>
                                    </Alert>
                                </div>
                            {:else}
                                <div
                                    class="h-[calc(100%-2.25rem)] w-full overflow-scroll [&>*]:m-0"
                                >
                                    <CodeBlock
                                        language={json()}
                                        readOnly={true}
                                        value={activeReqRef.self.response &&
                                        activeReqRef.self.response.status
                                            ? HTTP_STATUS_DESCRIPTION[
                                                  activeReqRef.self.response.status
                                              ]
                                            : "Something went wrong."}
                                        class="size-full"
                                    />
                                </div>
                            {/if}
                        {/if}
                    </TabsContent>
                    <TabsContent value="cookies" class="m-0 size-full">
                        {#if activeReqRef.self.response?.cookies}
                            {@render keyValueTable(activeReqRef.self.response.cookies)}
                        {:else}
                            <div class="flex size-full items-center justify-center">
                                No cookies for you :(
                            </div>
                        {/if}
                    </TabsContent>
                    <TabsContent value="headers" class="m-0 size-full">
                        {#if activeReqRef.self.response?.headers}
                            {@render keyValueTable(activeReqRef.self.response.headers)}
                        {:else}
                            <div class="flex size-full items-center justify-center">
                                No headers received
                            </div>
                        {/if}
                    </TabsContent>
                </div>
            {/if}
        </Tabs>
    {/if}
</div>
