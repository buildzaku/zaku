<script lang="ts">
  import type { PaneAPI } from "paneforge";
  import { ChevronDownIcon, ChevronUpIcon } from "@lucide/svelte";
  import { json } from "@codemirror/lang-json";
  import { html } from "@codemirror/lang-html";
  import { xml } from "@codemirror/lang-xml";
  import type { LanguageSupport } from "@codemirror/language";

  import { REQUEST_BODY_TYPES } from "$lib/utils/constants";
  import { Button } from "$lib/components/primitives/button";
  import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
  } from "$lib/components/primitives/select";
  import { Tabs, TabsList, TabsTrigger, TabsContent } from "$lib/components/primitives/tabs";
  import { CodeBlock } from "$lib/components/code-block";
  import type { ValueOf } from "$lib/utils";
  import type { ReqCfg } from "$lib/bindings";
  import { Headers, Parameters } from ".";

  type Props = {
    pane: PaneAPI;
    isCollapsed: boolean;
    config: ReqCfg;
  };

  let { pane, isCollapsed = $bindable(), config = $bindable() }: Props = $props();

  const reqCfgTabs = [
    { value: "parameters", label: "Parameters" },
    { value: "headers", label: "Headers" },
    { value: "body", label: "Body" },
  ] as const;

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
</script>

{#if isCollapsed}
  <div
    class="bg-accent/30 flex h-8 w-full items-center justify-between border-y border-b-transparent"
  >
    <div class="flex size-full items-center justify-end pr-1">
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
  </div>
{:else}
  <Tabs value="parameters" class="size-full">
    <div class="bg-accent/30 flex h-8 w-full items-center justify-between border-y">
      <div class="px-1.5">
        <TabsList class="grid auto-cols-min grid-flow-col justify-start gap-2 p-0">
          {#each reqCfgTabs as tab (tab.value)}
            <TabsTrigger value={tab.value} class="text-xs">{tab.label}</TabsTrigger>
          {/each}
        </TabsList>
      </div>
      <div class="flex size-full items-center justify-end pr-1">
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
    </div>

    <div class="bg-background flex h-[calc(100%-32px)] w-full">
      <TabsContent value="parameters" class="m-0 size-full">
        <div class="bg-card h-full overflow-auto px-4 py-3">
          <p class="mb-3">Query Parameters</p>
          <Parameters bind:config />
        </div>
      </TabsContent>
      <TabsContent value="headers" class="m-0 size-full">
        <div class="bg-card h-full overflow-auto px-4 py-3">
          <p class="mb-3">Headers</p>
          <Headers bind:config />
        </div>
      </TabsContent>
      <TabsContent value="body" class="m-0 size-full">
        <div class="bg-card flex h-9 items-center justify-start gap-3 border-b px-3">
          <span>Content Type</span>
          <Select
            type="single"
            bind:value={() => config.content_type ?? "", value => (config.content_type = value)}
          >
            <SelectTrigger class="w-fit">
              <span class="pr-3">
                {!config.content_type ? REQUEST_BODY_TYPES.None : config.content_type}
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

        {#if config.content_type && config.content_type !== "None"}
          <CodeBlock
            bind:language
            bind:value={() => config.body ?? "", value => (config.body = value)}
            class="bg-card h-full max-h-[calc(100%-2.25rem)] overflow-auto"
          />
        {/if}
      </TabsContent>
    </div>
  </Tabs>
{/if}
