<script lang="ts">
    import type { PaneAPI } from "paneforge";

    import { Button } from "$lib/components/primitives/button";
    import { Input } from "$lib/components/primitives/input";
    import {
        ResizablePaneGroup,
        ResizablePane,
        ResizableHandle,
    } from "$lib/components/primitives/resizable";
    import { SelectMethod } from "$lib/components/select-method";
    import { Sidebar } from "$lib/components/sidebar";
    import { ConfigurationPane } from "$lib/components/configuration-pane";
    import { ResponsePane } from "$lib/components/response-pane";
    import { cn } from "$lib/utils/style";
    import { treeItemsState, debounced, zakuState, baseRequestHeaders } from "$lib/state.svelte";
    import { joinPaths } from "$lib/components/tree-item/utils.svelte";
    import { REQUEST_BODY_TYPES } from "$lib/utils/constants";
    import { commands } from "$lib/bindings";

    let leftPane: PaneAPI | undefined = $state();
    let isLeftPaneCollapsed = $state(false);
    let cfgPane: PaneAPI | undefined = $state();
    let isReqPaneCollapsed = $state(false);
    let resPane: PaneAPI | undefined = $state();
    let isResPaneCollapsed = $state(false);

    async function handleSend() {
        const activeReqRef = treeItemsState.activeRequest;
        if (!activeReqRef) return;

        activeReqRef.self.status = "Pending";

        const validProtocol = /^(https?:\/\/)/i;

        if (!validProtocol.test(activeReqRef.self.config.url.raw ?? "")) {
            activeReqRef.self.status = "Error";
            activeReqRef.self.response = {
                status: undefined,
                data: "Invalid or missing protocol",
                headers: [],
                elapsed_ms: undefined,
            };
            return;
        }

        const url = new URL(activeReqRef.self.config.url.raw ?? "");
        activeReqRef.self.config.parameters?.forEach(([include, key, value]) => {
            if (include && !url.searchParams.has(key)) {
                url.searchParams.set(key, value);
            }
        });

        const requestHeaders = [
            ...baseRequestHeaders,
            ...(activeReqRef.self.config.headers ?? []),
        ].reduce((acc: Record<string, string>, [include, key, value]) => {
            if (include && !(key in acc)) {
                acc[key] = value;
            }
            return acc;
        }, {});

        if (
            activeReqRef.self.config.content_type &&
            activeReqRef.self.config.content_type !== REQUEST_BODY_TYPES.None
        ) {
            const hasContentType = Object.keys(requestHeaders).some(
                k => k.toLowerCase() === "content-type",
            );
            if (!hasContentType) {
                requestHeaders["Content-Type"] = activeReqRef.self.config.content_type;
            }
        }

        const reqUrl = {
            raw: url.href,
            protocol: url.protocol.replace(":", ""),
            host: url.hostname,
            path: url.pathname,
        };

        const reqPayload = {
            meta: activeReqRef.self.meta,
            config: {
                ...activeReqRef.self.config,
                url: reqUrl,
            },
            status: activeReqRef.self.status,
            response: null,
        };

        const httpRes = await commands.httpReq(reqPayload);

        if (httpRes.status === "error") {
            activeReqRef.self.status = "Error";
            activeReqRef.self.response = {
                status: undefined,
                data: httpRes.error.message,
                headers: [],
                elapsed_ms: undefined,
            };
            return;
        }

        activeReqRef.self.response = httpRes.data;
        activeReqRef.self.status =
            httpRes.data.status && httpRes.data.status >= 200 && httpRes.data.status < 300
                ? "Success"
                : "Error";
    }

    async function handleSave(event: KeyboardEvent) {
        const activeSpaceRef = zakuState.activeSpace;
        const activeReqRef = treeItemsState.activeRequest;
        if (!activeSpaceRef || !activeReqRef) {
            return;
        }

        if ((event.metaKey || event.ctrlKey) && event.key === "s") {
            event.preventDefault();

            const absoluteReqPath = joinPaths([
                activeSpaceRef.absolute_path,
                activeReqRef.parentRelativePath,
                activeReqRef.self.meta.file_name,
            ]);

            await debounced.flush(absoluteReqPath);
            await commands.writeBufferRequestToFs(
                activeSpaceRef.absolute_path,
                joinPaths([activeReqRef.parentRelativePath, activeReqRef.self.meta.file_name]),
            );

            isActiveReqSavedToFs = true;
            activeReqRef.self.meta.has_unsaved_changes = false;
        }
    }

    const activeSpaceRef = treeItemsState.activeRequest;
    let isActiveReqSavedToFs = false;
    let prevActiveReqRelPath = activeSpaceRef
        ? `${activeSpaceRef.parentRelativePath}/${activeSpaceRef.self.meta.file_name}`
        : null;

    $effect(() => {
        // Important hack to keep the effect deeply reactive
        JSON.stringify(treeItemsState.activeRequest);

        const activeSpaceRef = zakuState.activeSpace;
        const activeReqRef = treeItemsState.activeRequest;

        if (isActiveReqSavedToFs) {
            isActiveReqSavedToFs = false;
            return;
        }

        const activeReqRelPath = activeReqRef
            ? `${activeReqRef.parentRelativePath}/${activeReqRef.self.meta.file_name}`
            : null;

        if (
            activeSpaceRef &&
            activeReqRef &&
            prevActiveReqRelPath &&
            prevActiveReqRelPath === activeReqRelPath
        ) {
            debounced.saveRequestToBuffer(activeSpaceRef.absolute_path, activeReqRef);
            activeReqRef.self.meta.has_unsaved_changes = true;
        } else {
            prevActiveReqRelPath = activeReqRelPath;
        }
    });
</script>

<svelte:document onkeydown={handleSave} />

<div class="flex size-full flex-col items-center justify-center gap-4">
    <ResizablePaneGroup direction="horizontal" class="w-full">
        <ResizablePane
            bind:this={leftPane}
            defaultSize={15}
            minSize={15}
            maxSize={50}
            collapsedSize={5}
            collapsible={true}
            onCollapse={() => (isLeftPaneCollapsed = true)}
            onExpand={() => (isLeftPaneCollapsed = false)}
            class={cn(isLeftPaneCollapsed && "w-9 max-w-9 min-w-9")}
        >
            <Sidebar pane={leftPane} bind:isCollapsed={isLeftPaneCollapsed} />
        </ResizablePane>
        <ResizablePane
            defaultSize={50}
            class="bg-card relative mr-1.5 mb-1.5 rounded-md border border-l-0"
        >
            <ResizableHandle withHandle class="absolute z-10 h-full" />
            {@const activeReqRef = treeItemsState.activeRequest}
            {#if activeReqRef}
                <ResizablePaneGroup direction="vertical" class="size-full">
                    <div class="p-3">
                        <div class="mb-3 flex">
                            {activeReqRef.self.meta.display_name}
                        </div>
                        <div>
                            <form class="flex gap-2">
                                <SelectMethod bind:selected={activeReqRef.self.config.method} />
                                <Input
                                    bind:value={activeReqRef.self.config.url.raw}
                                    type="text"
                                    class="font-mono text-xs"
                                />
                                <Button type="submit" onclick={handleSend}>Send</Button>
                            </form>
                        </div>
                    </div>
                    <ResizablePane
                        bind:this={cfgPane}
                        defaultSize={25}
                        minSize={20}
                        collapsedSize={5.5}
                        collapsible={true}
                        onCollapse={() => {
                            isReqPaneCollapsed = true;
                        }}
                        onExpand={() => {
                            isReqPaneCollapsed = false;
                        }}
                        class={cn(isReqPaneCollapsed && "h-8 max-h-8 min-h-8")}
                    >
                        <ConfigurationPane
                            pane={cfgPane}
                            bind:isCollapsed={isReqPaneCollapsed}
                            bind:config={activeReqRef.self.config}
                        />
                    </ResizablePane>
                    <ResizableHandle withHandle />
                    <ResizablePane
                        bind:this={resPane}
                        defaultSize={75}
                        minSize={20}
                        collapsedSize={5}
                        collapsible={true}
                        onCollapse={() => {
                            isResPaneCollapsed = true;
                        }}
                        onExpand={() => {
                            isResPaneCollapsed = false;
                        }}
                        class={cn(isResPaneCollapsed && "h-8 max-h-8 min-h-8 ")}
                    >
                        <ResponsePane
                            pane={resPane}
                            isCollapsed={isResPaneCollapsed}
                            {activeReqRef}
                        />
                    </ResizablePane>
                </ResizablePaneGroup>
            {/if}
        </ResizablePane>
    </ResizablePaneGroup>
</div>
