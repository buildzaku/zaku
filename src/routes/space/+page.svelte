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
    import { treeNodesState, debounced, sharedState, baseRequestHeaders } from "$lib/state.svelte";
    import { joinPaths } from "$lib/components/tree-item/utils.svelte";
    import { REQUEST_BODY_TYPES } from "$lib/utils/constants";
    import { commands } from "$lib/bindings";
    import type { HttpReq, ReqUrl } from "$lib/bindings";

    let leftPane: PaneAPI | undefined = $state();
    let isLeftPaneCollapsed = $state(false);
    let cfgPane: PaneAPI | undefined = $state();
    let isReqPaneCollapsed = $state(false);
    let resPane: PaneAPI | undefined = $state();
    let isResPaneCollapsed = $state(false);

    async function handleSend() {
        const activeReqRef = treeNodesState.activeRequest;
        if (!activeReqRef) return;

        activeReqRef.self.status = "Pending";
        const validProtocol = /^(https?:\/\/)/i;
        if (!validProtocol.test(activeReqRef.self.config.url.raw ?? "")) {
            activeReqRef.self.status = "Error";
            activeReqRef.self.response = {
                data: "Invalid or missing protocol",
                headers: [],
                cookies: [],
            };
            return;
        }

        const url = new URL(activeReqRef.self.config.url.raw ?? "");

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

        const reqUrl: ReqUrl = {
            raw: url.href,
            protocol: url.protocol.replace(":", ""),
            host: url.hostname,
            path: url.pathname,
        };
        const req: HttpReq = {
            meta: activeReqRef.self.meta,
            config: {
                ...activeReqRef.self.config,
                url: reqUrl,
            },
            status: activeReqRef.self.status,
            response: null,
        };

        const httpRes = await commands.httpReq(req);

        if (sharedState.activeSpace) {
            const cookiesResult = await commands.getSpaceCookies(sharedState.activeSpace.abspath);

            if (cookiesResult.status === "ok") {
                sharedState.activeSpace.cookies = cookiesResult.data;
            }
        }

        if (httpRes.status === "error") {
            activeReqRef.self.status = "Error";
            activeReqRef.self.response = {
                data: httpRes.error.message,
                headers: [],
                cookies: [],
            };
        } else {
            activeReqRef.self.response = httpRes.data;
            activeReqRef.self.status =
                httpRes.data.status && httpRes.data.status >= 200 && httpRes.data.status < 300
                    ? "Success"
                    : "Error";
        }
    }

    async function handleSave(event: KeyboardEvent) {
        const activeSpaceRef = sharedState.activeSpace;
        const activeReqRef = treeNodesState.activeRequest;
        if (!activeSpaceRef || !activeReqRef) {
            return;
        }

        if ((event.metaKey || event.ctrlKey) && event.key === "s") {
            event.preventDefault();

            const absoluteReqPath = joinPaths([
                activeSpaceRef.abspath,
                activeReqRef.parentRelativePath,
                activeReqRef.self.meta.file_name,
            ]);

            await debounced.flush(absoluteReqPath);
            await commands.writeReqbufToReqtoml(
                activeSpaceRef.abspath,
                joinPaths([activeReqRef.parentRelativePath, activeReqRef.self.meta.file_name]),
            );

            isActiveReqSavedToFs = true;
            activeReqRef.self.meta.has_unsaved_changes = false;
        }
    }

    const activeSpaceRef = treeNodesState.activeRequest;
    let isActiveReqSavedToFs = false;
    let prevActiveReqRelPath = activeSpaceRef
        ? `${activeSpaceRef.parentRelativePath}/${activeSpaceRef.self.meta.file_name}`
        : null;

    $effect(() => {
        // Important hack to keep the effect deeply reactive
        JSON.stringify(treeNodesState.activeRequest);

        const activeSpaceRef = sharedState.activeSpace;
        const activeReqRef = treeNodesState.activeRequest;

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
            debounced.saveRequestToBuffer(activeSpaceRef.abspath, activeReqRef);
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

        <!-- align-marker: mt-px to align with sidebar's pt-px -->
        <ResizablePane
            defaultSize={50}
            class="bg-card relative mt-px mr-1.5 mb-1.5 rounded-md border border-l-0"
        >
            <ResizableHandle withHandle class="absolute z-10 h-full" />
            {@const activeReqRef = treeNodesState.activeRequest}
            {#if activeReqRef}
                <ResizablePaneGroup direction="vertical" class="size-full">
                    <div class="p-3">
                        <div class="mb-3 flex">
                            {activeReqRef.self.meta.name}
                        </div>
                        <div>
                            <form class="flex gap-2">
                                <SelectMethod bind:selected={activeReqRef.self.config.method} />
                                <Input
                                    bind:value={activeReqRef.self.config.url.raw}
                                    type="text"
                                    class="font-mono text-xs"
                                />
                                <Button
                                    type="submit"
                                    disabled={activeReqRef.self.status === "Pending"}
                                    onclick={handleSend}>Send</Button
                                >
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
                            activeReq={activeReqRef}
                        />
                    </ResizablePane>
                </ResizablePaneGroup>
            {/if}
        </ResizablePane>
    </ResizablePaneGroup>
</div>
