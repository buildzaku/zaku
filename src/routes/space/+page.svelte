<script lang="ts">
    import { fetch } from "@tauri-apps/plugin-http";

    import { Button } from "$lib/components/primitives/button";
    import { Input } from "$lib/components/primitives/input";
    import { BASE_REQUEST_HEADERS, type RequestStatus } from "$lib/utils/api";
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
    import type { PaneAPI } from "paneforge";
    import { treeItemsState, debounced, zakuState } from "$lib/state.svelte";
    import { safeInvoke } from "$lib/commands";
    import { joinPaths } from "$lib/components/tree-item/utils.svelte";

    let requestStatus: RequestStatus = $state("idle");
    let json = $state("");
    let error = $state("");
    let iframeSrcDoc = $state("");

    let leftPane: PaneAPI | undefined = $state();
    let isLeftPaneCollapsed = $state(false);
    let configurationPane: PaneAPI | undefined = $state();
    let isRequestPaneCollapsed = $state(false);
    let responsePane: PaneAPI | undefined = $state();
    let isResponsePaneCollapsed = $state(false);

    async function handleSend() {
        try {
            if (!treeItemsState.activeRequest) return;
            requestStatus = "loading";

            const validProtocol = new RegExp(/^(https?:\/\/)/i);
            if (!validProtocol.test(treeItemsState.activeRequest.self.config.url ?? "")) {
                throw new Error("Invalid or missing Protocol");
            }

            const url = new URL(treeItemsState.activeRequest.self.config.url ?? "");

            treeItemsState.activeRequest.self.config.parameters.reduceRight((acc, cur) => {
                const [include, key, value] = cur;
                if (include && !url.searchParams.has(key)) {
                    url.searchParams.set(key, value);
                }

                return acc;
            }, []);

            const response = await fetch(url, {
                method: treeItemsState.activeRequest.self.config.method,
                headers: [
                    ...BASE_REQUEST_HEADERS,
                    ...treeItemsState.activeRequest.self.config.headers,
                ].reduceRight((acc: Record<string, string>, cur) => {
                    const [include, key, value] = cur;
                    if (include && !(key in acc)) {
                        acc[key] = value;
                    }
                    return acc;
                }, {}),
            });

            if (!response.ok) {
                throw new Error(`${response.status}`);
            }

            json = await response.text();
            iframeSrcDoc = json;

            requestStatus = "success";
        } catch (err) {
            requestStatus = "error";
            if (err instanceof Error) {
                error = err.message;
            } else {
                error = "Not found";
            }
        }
    }

    async function handleSave(event: KeyboardEvent) {
        if (!zakuState.activeSpace || !treeItemsState.activeRequest) {
            return;
        }
        if ((event.metaKey || event.ctrlKey) && event.key === "s") {
            event.preventDefault();

            const absoluteRequestPath = joinPaths([
                zakuState.activeSpace.absolute_path,
                treeItemsState.activeRequest.parentRelativePath,
                treeItemsState.activeRequest.self.meta.file_name,
            ]);

            await debounced.flush(absoluteRequestPath);
            await safeInvoke("write_buffer_request_to_fs", {
                absolute_space_path: zakuState.activeSpace.absolute_path,
                request_relative_path: joinPaths([
                    treeItemsState.activeRequest.parentRelativePath,
                    treeItemsState.activeRequest.self.meta.file_name,
                ]),
            });

            isActiveRequestSavedToFs = true;
            treeItemsState.activeRequest.self.meta.has_unsaved_changes = false;
        }
    }

    let isActiveRequestSavedToFs = false;
    let previousActiveRequestRelativePath = treeItemsState.activeRequest
        ? `${treeItemsState.activeRequest.parentRelativePath}/${treeItemsState.activeRequest.self.meta.file_name}`
        : null;

    $effect(() => {
        // Important hack to keep the effect deeply reactive
        JSON.stringify(treeItemsState.activeRequest);

        if (isActiveRequestSavedToFs) {
            isActiveRequestSavedToFs = false;
            return;
        }

        const currentActiveRequestRelativePath = treeItemsState.activeRequest
            ? `${treeItemsState.activeRequest.parentRelativePath}/${treeItemsState.activeRequest.self.meta.file_name}`
            : null;

        if (
            zakuState.activeSpace &&
            treeItemsState.activeRequest &&
            previousActiveRequestRelativePath &&
            previousActiveRequestRelativePath === currentActiveRequestRelativePath
        ) {
            debounced.saveRequestToBuffer(
                zakuState.activeSpace.absolute_path,
                treeItemsState.activeRequest,
            );
            treeItemsState.activeRequest.self.meta.has_unsaved_changes = true;
        } else {
            previousActiveRequestRelativePath = currentActiveRequestRelativePath;
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
            {#if treeItemsState.activeRequest}
                <ResizablePaneGroup direction="vertical" class="size-full">
                    <div class="p-3">
                        <div class="mb-3 flex">
                            {treeItemsState.activeRequest.self.meta.display_name}
                        </div>
                        <div>
                            <form class="flex gap-2">
                                <SelectMethod
                                    bind:selected={treeItemsState.activeRequest.self.config.method}
                                />
                                <Input
                                    bind:value={treeItemsState.activeRequest.self.config.url}
                                    type="text"
                                    class="font-mono text-xs"
                                />
                                <Button type="submit" onclick={handleSend}>Send</Button>
                            </form>
                        </div>
                    </div>
                    <ResizablePane
                        bind:this={configurationPane}
                        defaultSize={25}
                        minSize={20}
                        collapsedSize={5.5}
                        collapsible={true}
                        onCollapse={() => {
                            isRequestPaneCollapsed = true;
                        }}
                        onExpand={() => {
                            isRequestPaneCollapsed = false;
                        }}
                        class={cn(isRequestPaneCollapsed && "h-8 max-h-8 min-h-8")}
                    >
                        <ConfigurationPane
                            pane={configurationPane}
                            bind:isCollapsed={isRequestPaneCollapsed}
                            bind:parameters={treeItemsState.activeRequest.self.config.parameters}
                            bind:headers={treeItemsState.activeRequest.self.config.headers}
                        />
                    </ResizablePane>
                    <ResizableHandle withHandle />
                    <ResizablePane
                        bind:this={responsePane}
                        defaultSize={75}
                        minSize={20}
                        collapsedSize={5}
                        collapsible={true}
                        onCollapse={() => {
                            isResponsePaneCollapsed = true;
                        }}
                        onExpand={() => {
                            isResponsePaneCollapsed = false;
                        }}
                        class={cn(isResponsePaneCollapsed && "h-8 max-h-8 min-h-8")}
                    >
                        <ResponsePane
                            pane={responsePane}
                            bind:isCollapsed={isResponsePaneCollapsed}
                            bind:status={requestStatus}
                            bind:raw={json}
                            bind:preview={iframeSrcDoc}
                            bind:error
                        />
                    </ResizablePane>
                </ResizablePaneGroup>
            {/if}
        </ResizablePane>
    </ResizablePaneGroup>
</div>
