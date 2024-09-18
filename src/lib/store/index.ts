import { writable } from "svelte/store";
import { null as vNull } from "valibot";
import type { InferInput } from "valibot";

import { REQUEST_BODY_TYPES } from "$lib/utils/constants";
import { Ok, Struct } from "$lib/utils/struct";
import type { ValueOf } from "$lib/utils/struct";

import { tick } from "svelte";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "svelte-sonner";
import { getSpaceReference, safeInvoke } from "$lib/commands";

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

export const StoreKey = {
    CurrentSpacePath: "active_space_path",
    SpacePaths: "space_paths",
};
const spaceMetaStruct = Struct.strictObject({
    name: Struct.string(),
});

const spaceConfigStruct = Struct.strictObject({
    meta: spaceMetaStruct,
});

const requestStruct = Struct.strictObject({
    name: Struct.string(),
});

const collectionStruct = Struct.strictObject({
    name: Struct.string(),
    requests: Struct.array(requestStruct),
});

const spaceStruct = Struct.strictObject({
    path: Struct.string(),
    config: spaceConfigStruct,
    collections: Struct.array(collectionStruct),
    requests: Struct.array(requestStruct),
});

export type Space = InferInput<typeof spaceStruct>;

export const spaceReferenceStruct = Struct.strictObject({
    path: Struct.string(),
    name: Struct.string(),
});

export type SpaceReference = InferInput<typeof spaceReferenceStruct>;

const zakuStateStruct = Struct.strictObject({
    active_space: Struct.nullable(spaceStruct),
    space_references: Struct.array(spaceReferenceStruct),
});

export type ZakuState = InferInput<typeof zakuStateStruct>;

export const createSpaceDtoStruct = Struct.strictObject({
    name: Struct.string(),
    location: Struct.string(),
});

export type CreateSpaceDto = InferInput<typeof createSpaceDtoStruct>;

export const zakuErrorStruct = Struct.strictObject({
    error: Struct.string(),
    message: Struct.string(),
});

export type ZakuError = InferInput<typeof zakuErrorStruct>;

function createZakuState() {
    const { set, subscribe } = writable<ZakuState>({ active_space: null, space_references: [] });

    async function synchronize() {
        const getZakuStateResult = await safeInvoke(zakuStateStruct, "get_zaku_state");

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
            const setActiveSpaceResult = await safeInvoke(vNull(), "set_active_space", {
                spaceReference: spaceReference,
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
    };
}

export const zakuState = createZakuState();

export async function createSpace(createSpaceDto: CreateSpaceDto) {
    const createSpaceRawResult = await invoke("create_space", {
        createSpaceDto,
    });
    const spaceReference = Struct.parse(spaceReferenceStruct, createSpaceRawResult);

    await zakuState.setActiveSpace(spaceReference);

    return;
}
