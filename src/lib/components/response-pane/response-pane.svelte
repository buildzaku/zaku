<script lang="ts">
    import { RefreshCwIcon, RocketIcon } from "@lucide/svelte";
    import type { PaneAPI } from "paneforge";
    import { ChevronDownIcon, ChevronUpIcon } from "@lucide/svelte";
    import { json } from "@codemirror/lang-json";

    import { Tabs, TabsList, TabsTrigger, TabsContent } from "$lib/components/primitives/tabs";
    import { CodeBlock } from "$lib/components/code-block";
    import { Button } from "$lib/components/primitives/button";
    import { Badge } from "$lib/components/primitives/badge";
    import { HTTP_STATUS_DESCRIPTION } from "$lib/utils/constants";
    import type { ActiveRequest } from "$lib/models";
    import type { HttpRes, SpaceCookie } from "$lib/bindings";
    import { prettyJson, formatSize, formatElapsed } from "$lib/utils";

    type Props = {
        pane: PaneAPI;
        isCollapsed: boolean;
        activeReq: ActiveRequest;
    };

    let { pane, isCollapsed, activeReq }: Props = $props();
</script>

{#snippet httpResMeta(httpRes: HttpRes)}
    <div class="mr-4.5 flex gap-1.5">
        {#if httpRes.status}
            <div class="flex items-center font-mono">
                {#if httpRes.status >= 200 && httpRes.status < 300}
                    <Badge variant="success">
                        <span class="cursor-default whitespace-nowrap select-text">
                            {httpRes.status}
                            {HTTP_STATUS_DESCRIPTION[httpRes.status] ?? ""}
                        </span>
                    </Badge>
                {:else}
                    <Badge variant="failure">
                        <span class="cursor-default whitespace-nowrap select-text">
                            {httpRes.status}
                            {HTTP_STATUS_DESCRIPTION[httpRes.status] ?? ""}
                        </span>
                    </Badge>
                {/if}
            </div>
        {/if}
        {#if httpRes.elapsed_ms}
            <div class="flex items-center gap-1.5 font-mono">
                <span class="text-foreground/35 text-[11px]">•</span>
                <span class="cursor-default text-[11px] whitespace-nowrap select-text">
                    {formatElapsed(httpRes.elapsed_ms)}
                </span>
            </div>
        {/if}
        {#if httpRes.size_bytes}
            <div class="flex items-center gap-1.5 font-mono">
                <span class="text-foreground/35 text-[11px]">•</span>
                <span class="cursor-default text-[11px] whitespace-nowrap select-text">
                    {formatSize(httpRes.size_bytes)}
                </span>
            </div>
        {/if}
    </div>
{/snippet}

{#snippet cookiesTable(cookies: SpaceCookie[])}
    <div class="m-3 h-full max-h-[calc(100%-1.5rem)]">
        <div class="bg-card flex h-full flex-col overflow-hidden rounded border">
            <div class="bg-accent/25 flex border-b font-semibold">
                <div class="w-[35%] max-w-[35%] border-r p-2">Key</div>
                <div class="flex-1 p-2">Value</div>
            </div>
            <div class="overflow-y-auto">
                {#each cookies as ck, idx (idx)}
                    <div class="flex border-b last:border-b-0">
                        <div class="w-[35%] max-w-[35%] border-r p-2 break-all whitespace-normal">
                            <span class="select-text">{ck.name}</span>
                        </div>
                        <div class="flex-1 p-2 break-all whitespace-normal">
                            <span class="select-text">{ck.value}</span>
                        </div>
                    </div>
                {/each}
            </div>
        </div>
    </div>
{/snippet}

{#snippet headersTable(headers: [string, string][])}
    <div class="m-3 h-full max-h-[calc(100%-1.5rem)]">
        <div class="bg-card flex h-full flex-col overflow-hidden rounded border">
            <div class="bg-accent/25 flex border-b font-semibold">
                <div class="w-[35%] max-w-[35%] border-r p-2">Key</div>
                <div class="flex-1 p-2">Value</div>
            </div>
            <div class="overflow-y-auto">
                {#each headers as [key, value], idx (idx)}
                    <div class="flex border-b last:border-b-0">
                        <div class="w-[35%] max-w-[35%] border-r p-2 break-all whitespace-normal">
                            <span class="select-text">{key}</span>
                        </div>
                        <div class="flex-1 p-2 break-all whitespace-normal">
                            <span class="select-text">{value}</span>
                        </div>
                    </div>
                {/each}
            </div>
        </div>
    </div>
{/snippet}

{#snippet responseBtn(collapsed: boolean)}
    <Button
        variant="ghost"
        onclick={() => {
            if (isCollapsed) {
                pane.expand();
                pane.resize(60);
            } else {
                pane.collapse();
            }
        }}
        class="mr-1 hover:bg-transparent"
    >
        <span class="pr-1.5 text-xs font-medium">Response</span>
        {#if collapsed}
            <ChevronUpIcon size={14} />
        {:else}
            <ChevronDownIcon size={14} />
        {/if}
    </Button>
{/snippet}

{#snippet responseBar()}
    <div class="bg-card flex h-8 w-full items-center justify-between border-y border-t-transparent">
        {#if isCollapsed}
            <div class="flex h-8 w-full items-center justify-end gap-1.5 border-b">
                {#if (activeReq.self.status === "Success" || activeReq.self.status === "Error") && activeReq.self.response}
                    {@render httpResMeta(activeReq.self.response)}
                {/if}
                {@render responseBtn(isCollapsed)}
            </div>
        {:else}
            {#if activeReq.self.status === "Success" || activeReq.self.status === "Error"}
                <div class="px-1.5">
                    <TabsList
                        class="grid auto-cols-min grid-flow-col justify-start gap-2 p-0 [&>*]:text-xs"
                    >
                        <TabsTrigger value="body">Body</TabsTrigger>
                        <TabsTrigger value="cookies">
                            {"Cookies".concat(
                                activeReq.self.response?.cookies
                                    ? ` (${activeReq.self.response?.cookies.length})`
                                    : "",
                            )}
                        </TabsTrigger>
                        <TabsTrigger value="headers">
                            {"Headers".concat(
                                activeReq.self.response?.headers
                                    ? ` (${activeReq.self.response?.headers.length})`
                                    : "",
                            )}
                        </TabsTrigger>
                    </TabsList>
                </div>
            {/if}

            <div class="flex h-8 w-full items-center justify-end gap-1.5 border-b">
                {#if (activeReq.self.status === "Success" || activeReq.self.status === "Error") && activeReq.self.response}
                    {@render httpResMeta(activeReq.self.response)}
                {/if}
                {@render responseBtn(isCollapsed)}
            </div>
        {/if}
    </div>
{/snippet}

<Tabs value="body" class="size-full">
    {@render responseBar()}

    {#if !isCollapsed}
        <div class="bg-background flex h-[calc(100%-32px)] w-full">
            <TabsContent value="body" class="m-0 size-full">
                {#if activeReq.self.status === "Idle"}
                    <div class="bg-card flex size-full items-center justify-center gap-2 pb-8">
                        <RocketIcon size="20" />
                        <span>
                            Hit <b class="font-semibold">Send</b> to make a request
                        </span>
                    </div>
                {:else if activeReq.self.status === "Pending"}
                    <div class="flex size-full items-center justify-center">
                        <RefreshCwIcon
                            strokeWidth={1.5}
                            absoluteStrokeWidth
                            size={20}
                            class="mr-3 animate-spin"
                        />
                    </div>
                {:else if activeReq.self.status === "Success" || activeReq.self.status === "Error"}
                    <Tabs value="pretty" class="bg-card size-full">
                        <div class="flex items-center justify-end border-b px-3">
                            <TabsList class="my-1 auto-cols-min grid-flow-col gap-2 p-0">
                                <TabsTrigger value="pretty">Pretty</TabsTrigger>
                                <TabsTrigger value="raw">Raw</TabsTrigger>
                                <TabsTrigger value="preview">Preview</TabsTrigger>
                            </TabsList>
                        </div>
                        <div class="h-[calc(100%-2.25rem)] w-full overflow-scroll [&>*]:m-0">
                            <TabsContent value="pretty" class="size-full">
                                <CodeBlock
                                    language={json()}
                                    readOnly={true}
                                    value={prettyJson(activeReq.self.response?.data)}
                                    class="size-full"
                                />
                            </TabsContent>
                            <TabsContent value="raw" class="size-full">
                                <CodeBlock
                                    language={null}
                                    readOnly={true}
                                    value={activeReq.self.response?.data}
                                    class="size-full"
                                />
                            </TabsContent>
                            <TabsContent value="preview" class="size-full">
                                <iframe
                                    title=""
                                    src="about:blank"
                                    srcdoc={activeReq.self.response
                                        ? activeReq.self.response.data
                                        : ""}
                                    class="size-full"
                                    loading="lazy"
                                    sandbox=""
                                ></iframe>
                            </TabsContent>
                        </div>
                    </Tabs>
                {/if}
            </TabsContent>
            <TabsContent value="cookies" class="m-0 size-full">
                {#if activeReq.self.response}
                    {@render cookiesTable(activeReq.self.response.cookies)}
                {:else}
                    <div class="flex size-full items-center justify-center">
                        No cookies for you :(
                    </div>
                {/if}
            </TabsContent>
            <TabsContent value="headers" class="m-0 size-full">
                {#if activeReq.self.response}
                    {@render headersTable(activeReq.self.response.headers)}
                {:else}
                    <div class="flex size-full items-center justify-center">
                        No headers received
                    </div>
                {/if}
            </TabsContent>
        </div>
    {/if}
</Tabs>
