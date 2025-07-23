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
    import { explorerState, debounced, sharedState, baseRequestHeaders } from "$lib/state.svelte";
    import { joinPaths } from "$lib/components/tree-node/utils.svelte";
    import { REQUEST_BODY_TYPES } from "$lib/utils/constants";
    import { commands } from "$lib/bindings";
    import type { HttpReq, ReqUrl } from "$lib/bindings";
    import { ChevronRightIcon, EllipsisIcon } from "@lucide/svelte";
    import { emitCmdError } from "$lib/utils";

    let leftPane: PaneAPI | undefined = $state();
    let isLeftPaneCollapsed = $state(false);
    let cfgPane: PaneAPI | undefined = $state();
    let isReqPaneCollapsed = $state(false);
    let resPane: PaneAPI | undefined = $state();
    let isResPaneCollapsed = $state(false);

    async function handleSend() {
        const openReqSnapshot = explorerState.openRequest;
        if (!openReqSnapshot) return;

        openReqSnapshot.self.status = "Pending";
        const validProtocol = /^(https?:\/\/)/i;
        if (!validProtocol.test(openReqSnapshot.self.config.url.raw ?? "")) {
            openReqSnapshot.self.status = "Error";
            openReqSnapshot.self.response = {
                data: "Invalid or missing protocol",
                headers: [],
                cookies: [],
            };
            return;
        }

        const url = new URL(openReqSnapshot.self.config.url.raw ?? "");

        const requestHeaders = [
            ...baseRequestHeaders,
            ...(openReqSnapshot.self.config.headers ?? []),
        ].reduce((acc: Record<string, string>, [include, key, value]) => {
            if (include && !(key in acc)) {
                acc[key] = value;
            }
            return acc;
        }, {});

        if (
            openReqSnapshot.self.config.content_type &&
            openReqSnapshot.self.config.content_type !== REQUEST_BODY_TYPES.None
        ) {
            const hasContentType = Object.keys(requestHeaders).some(
                k => k.toLowerCase() === "content-type",
            );
            if (!hasContentType) {
                requestHeaders["Content-Type"] = openReqSnapshot.self.config.content_type;
            }
        }

        const reqUrl: ReqUrl = {
            raw: url.href,
            protocol: url.protocol.replace(":", ""),
            host: url.hostname,
            path: url.pathname,
        };
        const req: HttpReq = {
            meta: openReqSnapshot.self.meta,
            config: {
                ...openReqSnapshot.self.config,
                url: reqUrl,
            },
            status: openReqSnapshot.self.status,
            response: null,
        };

        const httpRes = await commands.httpReq(req);

        if (sharedState.space) {
            const cookiesResult = await commands.getSpaceCookies(sharedState.space.abspath);

            if (cookiesResult.status === "ok") {
                sharedState.space.cookies = cookiesResult.data;
            }
        }

        if (httpRes.status === "error") {
            openReqSnapshot.self.status = "Error";
            openReqSnapshot.self.response = {
                data: httpRes.error.message,
                headers: [],
                cookies: [],
            };
        } else {
            openReqSnapshot.self.response = httpRes.data;
            openReqSnapshot.self.status =
                httpRes.data.status && httpRes.data.status >= 200 && httpRes.data.status < 300
                    ? "Success"
                    : "Error";
        }
    }

    async function handleSave(event: KeyboardEvent) {
        const spaceSnapshot = sharedState.space;
        const openReqSnapshot = explorerState.openRequest;
        if (!spaceSnapshot || !openReqSnapshot) {
            return;
        }

        if ((event.metaKey || event.ctrlKey) && event.key === "s") {
            event.preventDefault();

            const absoluteReqPath = joinPaths([
                spaceSnapshot.abspath,
                openReqSnapshot.parentRelpath,
                openReqSnapshot.self.meta.fsname,
            ]);

            await debounced.flush(absoluteReqPath);
            const writeReqbufToReqtomlResult = await commands.writeReqbufToReqtoml(
                spaceSnapshot.abspath,
                joinPaths([openReqSnapshot.parentRelpath, openReqSnapshot.self.meta.fsname]),
            );
            if (writeReqbufToReqtomlResult.status !== "ok") {
                return emitCmdError(writeReqbufToReqtomlResult.error);
            }

            isActiveReqSavedToFs = true;
            openReqSnapshot.self.meta.has_unsaved_changes = false;
        }
    }

    const spaceSnapshot = explorerState.openRequest;
    let isActiveReqSavedToFs = false;
    let prevActiveReqRelPath = spaceSnapshot
        ? `${spaceSnapshot.parentRelpath}/${spaceSnapshot.self.meta.fsname}`
        : null;

    $effect(() => {
        // Important hack to keep the effect deeply reactive
        JSON.stringify(explorerState.openRequest);

        const spaceSnapshot = sharedState.space;
        const openReqSnapshot = explorerState.openRequest;

        if (isActiveReqSavedToFs) {
            isActiveReqSavedToFs = false;
            return;
        }

        const openReqRelPath = openReqSnapshot
            ? `${openReqSnapshot.parentRelpath}/${openReqSnapshot.self.meta.fsname}`
            : null;

        if (
            spaceSnapshot &&
            openReqSnapshot &&
            prevActiveReqRelPath &&
            prevActiveReqRelPath === openReqRelPath
        ) {
            debounced.saveRequestToBuffer(spaceSnapshot.abspath, openReqSnapshot);
            openReqSnapshot.self.meta.has_unsaved_changes = true;
        } else {
            prevActiveReqRelPath = openReqRelPath;
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
            {@const openReqSnapshot = explorerState.openRequest}
            {#if openReqSnapshot}
                {@const MAX_PARENTS_TO_SHOW = 2}
                {@const parentsOverflow = openReqSnapshot.parentNames.length > MAX_PARENTS_TO_SHOW}
                <ResizablePaneGroup direction="vertical" class="size-full">
                    <div class="p-3">
                        <div class="mb-3 flex items-center gap-0.5">
                            {#if openReqSnapshot.parentNames.length > 0}
                                <span class="cursor-default select-text">
                                    {openReqSnapshot.parentNames[0]}
                                </span>
                                <ChevronRightIcon size={12} class="mx-0.5" />

                                {#if parentsOverflow}
                                    <EllipsisIcon size={12} />
                                    <ChevronRightIcon size={12} class="mx-0.5" />
                                {/if}

                                {#each openReqSnapshot.parentNames.slice(parentsOverflow ? -1 : 1) as parentName, idx (idx)}
                                    <span class="cursor-default select-text">{parentName}</span>
                                    <ChevronRightIcon size={12} class="mx-0.5" />
                                {/each}
                            {/if}
                            <span class="cursor-default select-text">
                                {openReqSnapshot.self.meta.name}
                            </span>
                        </div>
                        <div>
                            <form class="flex gap-2">
                                <SelectMethod bind:selected={openReqSnapshot.self.config.method} />
                                <Input
                                    bind:value={openReqSnapshot.self.config.url.raw}
                                    type="text"
                                    class="font-mono text-xs"
                                />
                                <Button
                                    type="submit"
                                    disabled={openReqSnapshot.self.status === "Pending"}
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
                            bind:config={openReqSnapshot.self.config}
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
                            openReq={openReqSnapshot}
                        />
                    </ResizablePane>
                </ResizablePaneGroup>
            {/if}
        </ResizablePane>
    </ResizablePaneGroup>
</div>
