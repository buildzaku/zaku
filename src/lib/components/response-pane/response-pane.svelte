<script lang="ts">
    import { Rocket } from "svelte-radix";

    import { Tabs, TabsList, TabsTrigger, TabsContent } from "$lib/components/primitives/tabs";
    import type { RequestStatus } from "$lib/utils/api";
    import { Alert } from "$lib/components/primitives/alert";
    import { CodeBlock } from "$lib/components/code-block";

    export let status: RequestStatus;
    export let raw: string;
    export let preview: string;
    export let error: string;
</script>

<div class="size-full bg-muted/25">
    {#if status === "idle"}
        <div class="flex size-full items-center justify-center gap-2">
            <Rocket size={20} />
            <span>
                Hit <b class="font-semibold">Send</b> to make a request
            </span>
        </div>
    {:else}
        <Tabs value="body" class="size-full">
            <div class="w-full border-b bg-muted/30 px-3 py-1">
                <TabsList
                    class="grid auto-cols-min grid-flow-col justify-start gap-2 p-0 [&>*]:text-xs"
                >
                    <TabsTrigger value="body">Body</TabsTrigger>
                    <TabsTrigger value="cookies">Cookies</TabsTrigger>
                    <TabsTrigger value="headers">Headers</TabsTrigger>
                </TabsList>
            </div>
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
                            <div class="h-[calc(100%-2.25rem)] w-full overflow-scroll [&>*]:m-0">
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
        </Tabs>
    {/if}
</div>
