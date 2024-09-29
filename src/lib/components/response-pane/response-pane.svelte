<script lang="ts">
    import { Rocket } from "svelte-radix";
    import type { PaneAPI } from "paneforge";
    import { ChevronDownIcon, ChevronUpIcon } from "lucide-svelte";

    import { Tabs, TabsList, TabsTrigger, TabsContent } from "$lib/components/primitives/tabs";
    import type { RequestStatus } from "$lib/utils/api";
    import { Alert } from "$lib/components/primitives/alert";
    import { CodeBlock } from "$lib/components/code-block";
    import { Button } from "$lib/components/primitives/button";

    export let pane: PaneAPI;
    export let isCollapsed: boolean;
    export let status: RequestStatus;
    export let raw: string;
    export let preview: string;
    export let error: string;
</script>

<div class="size-full bg-card">
    {#if status === "idle"}
        <div class="flex h-8 w-full items-center justify-between border-b bg-accent/25">
            <span class="px-3 text-xs font-medium">Response</span>
            <Button
                variant="ghost"
                on:click={() => {
                    if (isCollapsed) {
                        pane.expand();
                        pane.resize(60);
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

        {#if !isCollapsed}
            <div class="flex size-full items-center justify-center gap-2 pb-8">
                <Rocket size={20} />
                <span>
                    Hit <b class="font-semibold">Send</b> to make a request
                </span>
            </div>
        {/if}
    {:else}
        <Tabs value="body" class="size-full">
            <div
                class="flex h-8 w-full items-center justify-between border-y border-t-transparent bg-accent/25"
            >
                <div class="px-1.5">
                    <TabsList
                        class="grid auto-cols-min grid-flow-col justify-start gap-2 p-0 [&>*]:text-xs"
                    >
                        <TabsTrigger value="body">Body</TabsTrigger>
                        <TabsTrigger value="cookies">Cookies</TabsTrigger>
                        <TabsTrigger value="headers">Headers</TabsTrigger>
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
            {#if !isCollapsed}
                <div class="flex h-[calc(100%-2.25rem)] w-full items-center justify-center">
                    <TabsContent value="body" class="m-0 size-full">
                        {#if status === "success"}
                            <Tabs value="pretty" class="size-full">
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
                                    <TabsContent value="pretty">
                                        <CodeBlock lang="json" bind:value={raw} class="w-full" />
                                    </TabsContent>
                                    <TabsContent value="raw">
                                        <CodeBlock lang="json" bind:value={raw} class="size-fit" />
                                    </TabsContent>
                                    <TabsContent value="preview" class="size-full">
                                        <iframe
                                            title=""
                                            src="about:blank"
                                            srcdoc={preview}
                                            class="size-full"
                                            loading="lazy"
                                            sandbox=""
                                        />
                                    </TabsContent>
                                </div>
                            </Tabs>
                        {:else if status === "error"}
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
