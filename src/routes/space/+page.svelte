<script lang="ts">
    import { fetch } from "@tauri-apps/plugin-http";
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
    import { safeInvoke } from "$lib/commands";
    import { joinPaths } from "$lib/components/tree-item/utils.svelte";
    import { REQUEST_BODY_TYPES } from "$lib/utils/constants";

    let leftPane: PaneAPI | undefined = $state();
    let isLeftPaneCollapsed = $state(false);
    let cfgPane: PaneAPI | undefined = $state();
    let isReqPaneCollapsed = $state(false);
    let resPane: PaneAPI | undefined = $state();
    let isResPaneCollapsed = $state(false);

    async function handleSend() {
        const activeReqRef = treeItemsState.activeRequest;
        if (!activeReqRef) return;

        try {
            activeReqRef.self.status = "Pending";

            const validProtocol = new RegExp(/^(https?:\/\/)/i);
            if (!validProtocol.test(activeReqRef.self.config.url ?? "")) {
                throw new Error("Invalid or missing Protocol");
            }

            const url = new URL(activeReqRef.self.config.url ?? "");

            activeReqRef.self.config.parameters?.reduceRight((acc, cur) => {
                const [include, key, value] = cur;
                if (include && !url.searchParams.has(key)) {
                    url.searchParams.set(key, value);
                }

                return acc;
            }, []);

            const requestHeaders = [
                ...baseRequestHeaders,
                ...(activeReqRef?.self.config.headers ?? []),
            ].reduceRight((acc: Record<string, string>, cur) => {
                const [include, key, value] = cur;
                if (include && !(key in acc)) {
                    acc[key] = value;
                }
                return acc;
            }, {});

            const requestConfig: RequestInit = {
                method: activeReqRef.self.config.method,
                headers: requestHeaders,
            };

            if (
                activeReqRef.self.config.content_type &&
                activeReqRef.self.config.content_type !== REQUEST_BODY_TYPES.None
            ) {
                const hasContentType = Object.keys(requestHeaders).some(
                    key => key.toLowerCase() === "content-type",
                );
                if (!hasContentType) {
                    requestHeaders["Content-Type"] = activeReqRef.self.config.content_type;
                }

                requestConfig["body"] = activeReqRef.self.config.body;
            }

            const fetchResponse = await fetch(url, requestConfig);
            activeReqRef.self.response = {
                status: fetchResponse.status,
                data: String(),
            };

            if (fetchResponse.ok) {
                activeReqRef.self.response.data = await fetchResponse.text();
                activeReqRef.self.status = "Success";
            } else {
                activeReqRef.self.status = "Error";
            }
        } catch (err) {
            const errStr = String(err);
            const url = new URL(activeReqRef.self.config.url ?? "");
            activeReqRef.self.status = "Error";
            activeReqRef.self.response = {
                data: errStr.startsWith("error sending request for url")
                    ? `Error: connect ECONNREFUSED ${url.host}`
                    : errStr,
            };
        }
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
            await safeInvoke("write_buffer_request_to_fs", {
                absolute_space_path: activeSpaceRef.absolute_path,
                request_relative_path: joinPaths([
                    activeReqRef.parentRelativePath,
                    activeReqRef.self.meta.file_name,
                ]),
            });

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
                                    bind:value={activeReqRef.self.config.url}
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
