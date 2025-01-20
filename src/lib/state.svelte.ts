import { tick } from "svelte";
import { toast } from "svelte-sonner";

import { RELATIVE_SPACE_ROOT, REQUEST_BODY_TYPES } from "$lib/utils/constants";
import { Err, Ok } from "$lib/utils";
import type { ValueOf } from "$lib/utils";
import { safeInvoke } from "$lib/commands";
import { TREE_ITEM_TYPE, type DragPayload, type FocussedTreeItem } from "$lib/models";
import type { ZakuState as TZakuState, SpaceReference, Space } from "$lib/bindings";

export type RequestConfig = HttpRequestConfig;

export type Method =
    | "get"
    | "GET"
    | "delete"
    | "DELETE"
    | "head"
    | "HEAD"
    | "options"
    | "OPTIONS"
    | "post"
    | "POST"
    | "put"
    | "PUT"
    | "patch"
    | "PATCH"
    | "purge"
    | "PURGE"
    | "link"
    | "LINK"
    | "unlink"
    | "UNLINK";

export type MaybeKey<T extends string = string> = T | `_${T}`;

export type SerializedValue = string | number | boolean | null;

export type HttpRequestConfig = {
    name: string;
    type: "http";
    method: string;
    headers?: Record<MaybeKey, SerializedValue>;
    params?: Record<MaybeKey, SerializedValue>;
    body: {
        active: ValueOf<typeof REQUEST_BODY_TYPES>;
        "application/json": Record<string, SerializedValue>;
        "application/xml": string;
        "application/x-www-form-urlencoded": Record<string, SerializedValue>;
        // "application/octet-stream": ?,
        //  "multipart/form-data": ?,
        "text/html": string;
        "text/plain": string;
    };
};

class ZakuState {
    public activeSpace: Space | null = $state(null);
    public spaceReferences: SpaceReference[] = $state([]);

    public async synchronize() {
        const getZakuStateResult = await safeInvoke<TZakuState>("get_zaku_state");

        if (getZakuStateResult.ok) {
            this.activeSpace = getZakuStateResult.value.active_space;
            this.spaceReferences = getZakuStateResult.value.space_references;

            treeActionsState.reset();
            await tick();

            return Ok();
        } else {
            const { error, message } = getZakuStateResult.err;

            console.error(error);
            toast(message);

            return Err();
        }
    }

    public async setActiveSpace(spaceReference: SpaceReference) {
        const setActiveSpaceResult = await safeInvoke<null>("set_active_space", {
            space_reference: spaceReference,
        });

        if (setActiveSpaceResult.ok) {
            await this.synchronize();
        } else {
            const { error, message } = setActiveSpaceResult.err;

            console.error(error);
            toast(message);
        }

        return;
    }
}

export const zakuState = new ZakuState();

class TreeActionsState {
    #rootItem: FocussedTreeItem = {
        type: TREE_ITEM_TYPE.Collection,
        relativePath: RELATIVE_SPACE_ROOT,
        parentRelativePath: RELATIVE_SPACE_ROOT,
    };

    public dragPayload: DragPayload | null = $state(null);
    public dropTargetPath: string | null = $state(null);
    public focussedItem: FocussedTreeItem = $state(this.#rootItem);
    public createNewItem: ValueOf<typeof TREE_ITEM_TYPE> | null = $state(null);

    public reset() {
        this.dragPayload = null;
        this.dropTargetPath = null;
        this.focussedItem = this.#rootItem;
        this.createNewItem = null;
    }
}

export const treeActionsState = new TreeActionsState();
