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
    import type { HttpRes, SpaceCookie } from "$lib/bindings";

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

    function formatElapsed(ms: number): string {
        if (ms < 1000) return `${ms} ms`;

        const seconds = ms / 1000;
        if (seconds < 60) {
            return seconds % 1 === 0
                ? `${seconds}s`
                : `${seconds.toFixed(2).replace(/\.?0+$/, "")} s`;
        }

        const minutes = Math.floor(seconds / 60);
        const secRemainder = Math.floor(seconds % 60);
        if (minutes < 60) {
            return `${minutes} m ${secRemainder} s`;
        }

        const hours = Math.floor(minutes / 60);
        const minRemainder = minutes % 60;

        return `${hours} h ${minRemainder} m`;
    }

    function formatSize(bytes: number): string {
        if (bytes < 1024) return `${bytes} B`;

        const kb = bytes / 1024;
        if (kb < 1024) {
            return kb % 1 === 0 ? `${kb} KB` : `${kb.toFixed(2).replace(/\.?0+$/, "")} KB`;
        }

        const mb = kb / 1024;
        if (mb < 1024) {
            return mb % 1 === 0 ? `${mb} MB` : `${mb.toFixed(2).replace(/\.?0+$/, "")} MB`;
        }

        const gb = mb / 1024;

        return gb % 1 === 0 ? `${gb} GB` : `${gb.toFixed(2).replace(/\.?0+$/, "")} GB`;
    }
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
                        {#if activeReqRef.self.response}
                            {@render httpResMeta(activeReqRef.self.response)}
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
                        {#if activeReqRef.self.response}
                            {@render httpResMeta(activeReqRef.self.response)}
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
                        {#if activeReqRef.self.response}
                            {@render cookiesTable(activeReqRef.self.response.cookies)}
                        {:else}
                            <div class="flex size-full items-center justify-center">
                                No cookies for you :(
                            </div>
                        {/if}
                    </TabsContent>
                    <TabsContent value="headers" class="m-0 size-full">
                        {#if activeReqRef.self.response}
                            {@render headersTable(activeReqRef.self.response.headers)}
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
