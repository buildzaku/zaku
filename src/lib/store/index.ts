import { writable } from "svelte/store";

import { REQUEST_BODY_TYPES } from "$lib/utils/constants";
import { Struct, type InferInput, type ValueOf } from "$lib/utils/struct";

import { tick } from "svelte";
import { invoke } from "@tauri-apps/api/core";

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
    activeSpace: Struct.nullable(spaceStruct),
    spaceReferences: Struct.array(spaceReferenceStruct),
});

export type ZakuState = InferInput<typeof zakuStateStruct>;

const createSpaceDtoStruct = Struct.strictObject({
    name: Struct.string(),
    location: Struct.string(),
});

export type CreateSpaceDto = InferInput<typeof createSpaceDtoStruct>;

const zakuErrorStruct = Struct.strictObject({
    error: Struct.string(),
});

function createSpaceStore() {
    const { set, subscribe } = writable<Space | null>(null);

    async function synchronize() {
        const activeSpace: Space | null = await invoke("get_active_space");
        set(activeSpace);
        await tick();

        return;
    }

    return {
        synchronize,
        set: async (spaceReference: SpaceReference) => {
            await invoke("set_active_space", { spaceReference: spaceReference });
            await synchronize();

            return;
        },
        delete: async () => {
            await invoke("delete_active_space");
            await synchronize();

            return;
        },
        subscribe,
    };
}

export const activeSpace = createSpaceStore();

export async function createSpace(dto: CreateSpaceDto) {
    const createSpaceRawResult = await invoke("create_space", {
        createSpaceDto: dto,
    });
    const spaceReference = Struct.parse(spaceReferenceStruct, createSpaceRawResult);

    await activeSpace.set(spaceReference);

    return;
}

function createSpaceReferencesStore() {
    const { set, subscribe } = writable<SpaceReference[]>([]);

    async function synchronize() {
        const spaceReferences: SpaceReference[] = await invoke("get_saved_spaces");
        set(spaceReferences);
        await tick();

        return;
    }

    return {
        synchronize,
        // set: async (spaceReference: SpaceReference) => {
        //     await invoke("set_active_space", { spaceReference: spaceReference });
        //     await synchronize();

        //     return;
        // },
        // delete: async () => {
        //     await invoke("delete_active_space");
        //     await synchronize();

        //     return;
        // },
        subscribe,
    };
}

export const spaceReferences = createSpaceReferencesStore();
