<script lang="ts">
    import { RocketIcon } from "lucide-svelte";
    import type { PaneAPI } from "paneforge";
    import { ChevronDownIcon, ChevronUpIcon } from "lucide-svelte";

    import { Tabs, TabsList, TabsTrigger, TabsContent } from "$lib/components/primitives/tabs";
    import type { RequestStatus } from "$lib/utils/api";
    import { Alert } from "$lib/components/primitives/alert";
    import { CodeBlock } from "$lib/components/code-block";
    import { Button } from "$lib/components/primitives/button";

    type Props = {
        pane: PaneAPI;
        isCollapsed: boolean;
        status: RequestStatus;
        raw: string;
        preview: string;
        error: string;
    };

    let {
        pane,
        isCollapsed = $bindable(),
        status = $bindable(),
        raw = $bindable(),
        preview = $bindable(),
        error = $bindable(),
    }: Props = $props();
</script>

<div class="size-full bg-card">
    {#if status === "idle"}
        {#if isCollapsed}
            <div class="flex h-8 w-full items-center justify-between border-b bg-accent/25">
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
            <div class="flex h-8 w-full items-center justify-between border-b bg-accent/25">
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
    {:else}
        <Tabs value="body" class="size-full">
            <div
                class="flex h-8 w-full items-center justify-between border-y border-t-transparent bg-accent/25"
            >
                {#if isCollapsed}
                    <div class="flex h-8 w-full items-center justify-between border-b">
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
                    <div class="px-1.5">
                        <TabsList
                            class="grid auto-cols-min grid-flow-col justify-start gap-2 p-0 [&>*]:text-xs"
                        >
                            <TabsTrigger value="body">Body</TabsTrigger>
                            <TabsTrigger value="cookies">Cookies</TabsTrigger>
                            <TabsTrigger value="headers">Headers</TabsTrigger>
                        </TabsList>
                    </div>
                    <div class="flex h-8 w-full items-center justify-between border-b">
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
                {/if}
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
                                        ></iframe>
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
