import { writable } from "svelte/store";

import { REQUEST_BODY_TYPES } from "$lib/utils/constants";
import { Struct, type InferInput, type ValueOf } from "$lib/utils/struct";

import { tick } from "svelte";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "svelte-sonner";
import { getSpaceReference } from "$lib/commands";

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

const createSpaceDtoStruct = Struct.strictObject({
    name: Struct.string(),
    location: Struct.string(),
});

export type CreateSpaceDto = InferInput<typeof createSpaceDtoStruct>;

export const zakuErrorStruct = Struct.strictObject({
    error: Struct.string(),
});

function createZakuState() {
    const { set, subscribe } = writable<ZakuState>({ active_space: null, space_references: [] });

    async function synchronize() {
        try {
            const zakuStateRaw = await invoke("get_zaku_state");
            const zakuState = Struct.parse(zakuStateStruct, zakuStateRaw);
            set(zakuState);
            await tick();

            return;
        } catch (err) {
            console.error(err);
            toast("Unable to synchronize");
        }
    }

    return {
        initialize: synchronize,
        set: async (spaceReference: SpaceReference) => {
            try {
                await invoke("set_active_space", {
                    spaceReference: spaceReference,
                });
                await synchronize();

                return;
            } catch (err) {
                console.error(err);
            }
        },
        delete: async (path: string) => {
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

    await zakuState.set(spaceReference);

    return;
}
