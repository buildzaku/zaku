<script lang="ts">
    import type { PaneAPI } from "paneforge";
    import { ChevronDownIcon, ChevronUpIcon } from "lucide-svelte";
    import { json } from "@codemirror/lang-json";
    import { html } from "@codemirror/lang-html";
    import { xml } from "@codemirror/lang-xml";

    import { KeyValueList } from "$lib/components/key-value-list";
    import { Tabs, TabsList, TabsTrigger, TabsContent } from "$lib/components/primitives/tabs";
    import { REQUEST_BODY_TYPES } from "$lib/utils/constants";
    import { Button } from "$lib/components/primitives/button";
    import { cn } from "$lib/utils/style";
    import { Select, SelectContent, SelectItem, SelectTrigger } from "../primitives/select";
    import { CodeBlock } from "../code-block";
    import type { ValueOf } from "$lib/utils";
    import type { LanguageSupport } from "@codemirror/language";
    import type { RequestConfig } from "$lib/bindings";

    type Props = {
        pane: PaneAPI;
        class?: string;
        isCollapsed: boolean;
        config: RequestConfig;
    };

    let {
        pane,
        isCollapsed = $bindable(),
        config = $bindable(),
        class: className,
    }: Props = $props();

    let currentTab: "parameters" | "headers" | "body" = $state("parameters");
    let contentType = $derived(config.content_type ? config.content_type : "None");
    let language: LanguageSupport | null = $derived.by(() => {
        switch (config.content_type) {
            case REQUEST_BODY_TYPES.Json: {
                return json();
            }
            case REQUEST_BODY_TYPES.Xml: {
                return xml();
            }
            case REQUEST_BODY_TYPES.Html: {
                return html();
            }
            default: {
                return null;
            }
        }
    });
    function isBodyTypeDisabled(bodyType: ValueOf<typeof REQUEST_BODY_TYPES>) {
        return (
            bodyType === "application/octet-stream" ||
            bodyType === "application/x-www-form-urlencoded" ||
            bodyType === "multipart/form-data"
        );
    }

    $effect(() => {
        if (contentType) {
            config.content_type = contentType;
        }
    });
</script>

<div
    class={cn(
        "bg-accent/25 flex h-8 w-full items-center justify-between border-y",
        isCollapsed ? "border-b-transparent" : "",
    )}
>
    {#if isCollapsed}
        <div class="flex size-full items-center justify-end">
            <Button
                variant="ghost"
                onclick={() => {
                    pane.expand();
                    pane.resize(40);
                }}
                class="hover:bg-transparent"
            >
                <span class="pr-1.5 text-xs font-medium">Configuration</span>
                <ChevronDownIcon size={14} />
            </Button>
        </div>
    {:else}
        <div class="px-1.5">
            <div class="grid auto-cols-min grid-flow-col justify-start gap-2 p-0 [&>*]:text-xs">
                <Button
                    data-state={currentTab === "parameters" ? "active" : "inactive"}
                    class="text-small text-muted-foreground ring-offset-background focus-visible:ring-ring data-[state=active]:border-foreground/20 data-[state=active]:bg-muted data-[state=active]:text-foreground inline-flex h-6 cursor-pointer items-center justify-center rounded-md border border-transparent bg-transparent px-1.5 font-medium whitespace-nowrap transition-all hover:bg-transparent focus-visible:ring-1 focus-visible:outline-none disabled:pointer-events-none disabled:opacity-50 data-[state=active]:shadow"
                    onclick={() => (currentTab = "parameters")}>Parameters</Button
                >
                <Button
                    data-state={currentTab === "headers" ? "active" : "inactive"}
                    class="text-small text-muted-foreground ring-offset-background focus-visible:ring-ring data-[state=active]:border-foreground/20 data-[state=active]:bg-muted data-[state=active]:text-foreground inline-flex h-6 cursor-pointer items-center justify-center rounded-md border border-transparent bg-transparent px-1.5 font-medium whitespace-nowrap transition-all hover:bg-transparent focus-visible:ring-1 focus-visible:outline-none disabled:pointer-events-none disabled:opacity-50 data-[state=active]:shadow"
                    onclick={() => (currentTab = "headers")}>Headers</Button
                >
                <Button
                    data-state={currentTab === "body" ? "active" : "inactive"}
                    class="text-small text-muted-foreground ring-offset-background focus-visible:ring-ring data-[state=active]:border-foreground/20 data-[state=active]:bg-muted data-[state=active]:text-foreground inline-flex h-6 cursor-pointer items-center justify-center rounded-md border border-transparent bg-transparent px-1.5 font-medium whitespace-nowrap transition-all hover:bg-transparent focus-visible:ring-1 focus-visible:outline-none disabled:pointer-events-none disabled:opacity-50 data-[state=active]:shadow"
                    onclick={() => (currentTab = "body")}>Body</Button
                >
            </div>
        </div>
        <div class="flex size-full items-center justify-end">
            <Button
                variant="ghost"
                onclick={() => {
                    pane.collapse();
                }}
                class="hover:bg-transparent"
            >
                <span class="pr-1.5 text-xs font-medium">Configuration</span>
                <ChevronUpIcon size={14} />
            </Button>
        </div>
    {/if}
</div>

{#if currentTab === "parameters"}
    <div class="bg-card h-[calc(100%-2rem)] overflow-auto px-4 py-3">
        <p class="mb-3">Query Parameters</p>
        <KeyValueList type="parameter" bind:pairs={config.parameters} />
    </div>
{:else if currentTab === "headers"}
    <div class="bg-card h-[calc(100%-2rem)] overflow-auto px-4 py-3">
        <p class="mb-3">Headers</p>
        <KeyValueList type="header" bind:pairs={config.headers} />
    </div>
{:else if currentTab === "body"}
    <div class="flex h-9 items-center justify-start gap-3 border-b px-3">
        <span>Content Type</span>
        <Select type="single" bind:value={contentType}>
            <SelectTrigger class="w-fit">
                <span class="pr-3">
                    {contentType}
                </span>
            </SelectTrigger>
            <SelectContent align="start">
                {#each Object.values(REQUEST_BODY_TYPES) as BODY_TYPE (BODY_TYPE)}
                    <SelectItem value={BODY_TYPE} disabled={isBodyTypeDisabled(BODY_TYPE)}>
                        {BODY_TYPE}
                    </SelectItem>
                {/each}
            </SelectContent>
        </Select>
    </div>

    {#if contentType && contentType !== "None"}
        <CodeBlock
            bind:language
            bind:value={config.body}
            class="bg-card h-full max-h-[calc(100%-2rem-2.25rem)] overflow-auto"
        />
    {/if}
{/if}
