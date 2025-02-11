<script lang="ts">
    import { fetch } from "@tauri-apps/plugin-http";

    import { version } from "$app/environment";
    import { Button } from "$lib/components/primitives/button";
    import { Input } from "$lib/components/primitives/input";
    import type { KeyValuePair, RequestStatus } from "$lib/utils/api";
    import {
        ResizablePaneGroup,
        ResizablePane,
        ResizableHandle,
    } from "$lib/components/primitives/resizable";
    import { SelectMethod } from "$lib/components/select-method";
    import { METHODS } from "$lib/utils/constants";
    import { Sidebar } from "$lib/components/sidebar";
    import { ConfigurationPane } from "$lib/components/configuration-pane";
    import { ResponsePane } from "$lib/components/response-pane";
    import { cn } from "$lib/utils/style";
    import type { PaneAPI } from "paneforge";

    let requestStatus: RequestStatus = $state("idle");
    let currentUrl = $state("");
    let json = $state("");
    let error = $state("");
    let method: (typeof METHODS)[keyof typeof METHODS] = $state(METHODS.Get);
    let iframeSrcDoc = $state("");

    let leftPane: PaneAPI | undefined = $state();
    let isLeftPaneCollapsed = $state(false);
    let configurationPane: PaneAPI | undefined = $state();
    let isRequestPaneCollapsed = $state(false);
    let responsePane: PaneAPI | undefined = $state();
    let isResponsePaneCollapsed = $state(false);

    let currentRequestParams: KeyValuePair[] = $state([]);
    let currentRequestHeaders: KeyValuePair[] = $state([
        {
            key: "Cache-Control",
            value: "no-cache",
            include: true,
        },
        {
            key: "User-Agent",
            value: `Zaku/${version}`,
            include: true,
        },
    ]);

    async function handleSend() {
        try {
            requestStatus = "loading";

            const validProtocol = new RegExp(/^(https?:\/\/)/i);
            if (!validProtocol.test(currentUrl)) {
                throw new Error("Invalid or missing Protocol");
            }

            const url = new URL(currentUrl);

            currentRequestParams.reduceRight((acc, cur) => {
                if (cur.include && !url.searchParams.has(cur.key)) {
                    url.searchParams.set(cur.key, cur.value);
                }

                return acc;
            }, []);

            const response = await fetch(url, {
                method: method,
                headers: currentRequestHeaders.reduceRight((acc: Record<string, string>, cur) => {
                    if (cur.include && !(cur.key in acc)) {
                        acc[cur.key] = cur.value;
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
</script>

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
            class={cn(isLeftPaneCollapsed && "w-9 min-w-9 max-w-9")}
        >
            <Sidebar pane={leftPane} bind:isCollapsed={isLeftPaneCollapsed} />
        </ResizablePane>
        <ResizablePane
            defaultSize={50}
            class="relative mb-1.5 mr-1.5 rounded-md border border-l-0 bg-card"
        >
            <ResizableHandle withHandle class="absolute z-10 h-full" />
            <ResizablePaneGroup direction="vertical" class="size-full">
                <div class="p-3">
                    <div class="mb-3 flex">New HTTP request</div>
                    <div>
                        <form class="flex gap-2">
                            <SelectMethod bind:selected={method} />
                            <Input bind:value={currentUrl} type="text" class="font-mono text-xs" />
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
                        bind:parameters={currentRequestParams}
                        bind:headers={currentRequestHeaders}
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
        </ResizablePane>
    </ResizablePaneGroup>
</div>
