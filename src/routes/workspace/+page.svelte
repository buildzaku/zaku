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
    import { RequestConfigPane } from "$lib/components/request-config-pane";
    import { ResponsePane } from "$lib/components/response-pane";
    import { cn } from "$lib/utils/style";

    let requestStatus: RequestStatus = "idle";
    let currentUrl = "";
    let json = "";
    let error = "";
    let method = METHODS.GET;
    let iframeSrcDoc = "";

    let isLeftPaneCollapsed = false;

    let currentRequestParams: KeyValuePair[] = [];
    let currentRequestHeaders: KeyValuePair[] = [
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
    ];

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
                method: method.value,
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
            defaultSize={15}
            minSize={15}
            maxSize={33}
            collapsedSize={5}
            collapsible={true}
            onCollapse={() => (isLeftPaneCollapsed = true)}
            onExpand={() => (isLeftPaneCollapsed = false)}
            class={cn(isLeftPaneCollapsed && "w-10 max-w-10")}
        >
            <Sidebar bind:isCollapsed={isLeftPaneCollapsed} />
        </ResizablePane>
        <ResizablePane
            defaultSize={50}
            class="relative my-1.5 mr-1.5 rounded-md border border-l-0 bg-card"
        >
            <ResizableHandle withHandle class="absolute z-10 h-full" />
            <ResizablePaneGroup direction="vertical" class="size-full">
                <div class="p-3">
                    <div class="mb-3 flex">New HTTP request</div>
                    <div>
                        <form class="flex gap-2">
                            <SelectMethod bind:selected={method} />
                            <Input bind:value={currentUrl} type="text" class="font-mono text-xs" />
                            <Button type="submit" on:click={handleSend}>Send</Button>
                        </form>
                    </div>
                </div>
                <ResizablePane defaultSize={25} minSize={20} collapsedSize={5.5} collapsible={true}>
                    <RequestConfigPane
                        bind:parameters={currentRequestParams}
                        bind:headers={currentRequestHeaders}
                    />
                </ResizablePane>
                <ResizableHandle withHandle />
                <ResizablePane defaultSize={75} minSize={20} collapsedSize={5} collapsible={true}>
                    <ResponsePane
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
