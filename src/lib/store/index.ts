import { tick } from "svelte";
import { writable } from "svelte/store";
import { toast } from "svelte-sonner";

import { RELATIVE_SPACE_ROOT, REQUEST_BODY_TYPES } from "$lib/utils/constants";
import { Ok } from "$lib/utils";
import type { ValueOf } from "$lib/utils";
import { safeInvoke } from "$lib/commands";
import { TREE_ITEM_TYPE, type DragPayload, type FocussedTreeItem } from "$lib/models";
import type { ZakuState, SpaceReference } from "$lib/bindings";

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

function createZakuState() {
    const { set, subscribe, update } = writable<ZakuState>({
        active_space: null,
        space_references: [],
    });

    async function synchronize() {
        const getZakuStateResult = await safeInvoke<ZakuState>("get_zaku_state");

        if (getZakuStateResult.ok) {
            set(getZakuStateResult.value);
            currentDragPayload.reset();
            currentDropTargetPath.reset();
            focussedTreeItem.reset();
            createNewTreeItem.set(null);
            await tick();
        } else {
            const { error, message } = getZakuStateResult.err;

            console.error(error);
            toast(message);
        }

        return Ok();
    }

    return {
        synchronize,
        setActiveSpace: async (spaceReference: SpaceReference) => {
            const setActiveSpaceResult = await safeInvoke<null>("set_active_space", {
                space_reference: spaceReference,
            });

            if (setActiveSpaceResult.ok) {
                await synchronize();
            } else {
                const { error, message } = setActiveSpaceResult.err;

                console.error(error);
                toast(message);
            }

            return;
        },
        subscribe,
        update,
        set,
    };
}

export const zakuState = createZakuState();

function createDragPayloadState() {
    const { set, subscribe, update } = writable<DragPayload | null>(null);

    return {
        reset: () => set(null),
        set,
        subscribe,
        update,
    };
}

export const currentDragPayload = createDragPayloadState();

function currentDropTargetPathState() {
    const { set, subscribe, update } = writable<string | null>(null);

    return {
        reset: () => set(null),
        set,
        subscribe,
        update,
    };
}

export const currentDropTargetPath = currentDropTargetPathState();

function focussedTreeItemState() {
    const initialState: FocussedTreeItem = {
        type: TREE_ITEM_TYPE.Collection,
        relativePath: RELATIVE_SPACE_ROOT,
        parentRelativePath: RELATIVE_SPACE_ROOT,
    };
    const { set, subscribe, update } = writable<FocussedTreeItem>(initialState);

    return {
        reset: () => set(initialState),
        set,
        subscribe,
        update,
    };
}

export const focussedTreeItem = focussedTreeItemState();

export const createNewTreeItem = writable<ValueOf<typeof TREE_ITEM_TYPE> | null>(null);
