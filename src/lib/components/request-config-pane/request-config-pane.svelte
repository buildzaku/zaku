<script lang="ts">
    import { KeyValueList } from "$lib/components/key-value-list";
    import { Tabs, TabsList, TabsTrigger, TabsContent } from "$lib/components/primitives/tabs";
    import type { KeyValuePair } from "$lib/utils/api";
    import { REQUEST_BODY_TYPES } from "$lib/utils/constants";
    import { RequestBody } from "$lib/components/request-config-pane";

    export let parameters: KeyValuePair[];
    export let headers: KeyValuePair[];

    let body = REQUEST_BODY_TYPES.None;
</script>

<div class="size-full bg-muted/25">
    <Tabs value="parameters">
        <div class="w-full border-y bg-muted/30 px-3 py-1">
            <TabsList
                class="grid auto-cols-min grid-flow-col justify-start gap-2 p-0 [&>*]:text-xs"
            >
                <TabsTrigger value="parameters">Parameters</TabsTrigger>
                <TabsTrigger value="headers">Headers</TabsTrigger>
                <TabsTrigger value="body">Body</TabsTrigger>
            </TabsList>
        </div>
        <div>
            <TabsContent value="parameters" class="m-0 p-3">
                <div class="mb-3">Query Parameters</div>
                <KeyValueList type="parameter" bind:pairs={parameters} />
            </TabsContent>
            <TabsContent value="headers" class="m-0 p-3">
                <div class="mb-3">Headers</div>
                <KeyValueList type="header" bind:pairs={headers} />
            </TabsContent>
            <TabsContent value="body" class="m-0">
                <RequestBody bind:selected={body} />
            </TabsContent>
        </div>
    </Tabs>
</div>
