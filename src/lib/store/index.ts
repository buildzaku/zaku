import { tick } from "svelte";
import { writable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "svelte-sonner";

import { REQUEST_BODY_TYPES } from "$lib/utils/constants";
import { Ok } from "$lib/utils";
import type { ValueOf } from "$lib/utils";
import { getSpaceReference, safeInvoke } from "$lib/commands";
import type { DragPayload, CreateSpaceDto, SpaceReference, ZakuState } from "$lib/models";

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
            await tick();
        } else {
            const { error, message } = getZakuStateResult.err;

            console.error(error);
            toast(message);
        }

        return Ok();
    }

    return {
        initialize: synchronize,
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
        deleteSpace: async (path: string) => {
            try {
                const spaceReference = await getSpaceReference(path);
                await invoke("delete_space", {
                    spaceReference: spaceReference,
                });
                await synchronize();

                return;
            } catch (err) {
                console.error(err);
            }
        },
        subscribe,
        update,
        set,
    };
}

export const zakuState = createZakuState();

export async function createSpace(createSpaceDto: CreateSpaceDto) {
    const spaceReference = await invoke<SpaceReference>("create_space", {
        createSpaceDto,
    });

    await zakuState.setActiveSpace(spaceReference);

    return;
}

export const currentDragPayload = writable<DragPayload | null>(null);

export const currentDropTargetPath = writable<string | null>(null);
