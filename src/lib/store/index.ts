import { writable } from "svelte/store";

import { REQUEST_BODY_TYPES } from "$lib/utils/constants";
import { Struct, type ValueOf } from "$lib/utils/struct";

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

export type SpaceStoreDto = {
    path: string;
    name: string;
};

export type SpaceMeta = {
    name: string;
};

export type SpaceConfig = {
    meta: SpaceMeta;
};

export type Request = {
    name: string;
};

export type Collection = {
    name: string;
    requests: Request[];
};

export type Space = {
    path: string;
    config: SpaceConfig;
    collections: Collection[];
    requests: Request[];
};

type CreateSpaceDto = {
    path: string;
    name: string;
};

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
        set: async (path: string) => {
            console.log("invookdingng");
            await invoke("set_active_space", { path });
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
    try {
        const createSpaceResultSchema = Struct.strictObject({
            path: Struct.string(),
        });
        const createSpaceRawResult = await invoke("create_space", {
            createSpaceDto: dto,
        });

        const createSpaceResult = Struct.parse(createSpaceResultSchema, createSpaceRawResult);

        await activeSpace.set(createSpaceResult.path);

        return;
    } catch (error) {
        console.error("yoyoyoyoyoy createSpace");

        console.log(error);
    }
}

// function createSpacesStore() {
//     const { set, subscribe } = writable<Space[]>([]);

//     return {
//         set,
//         subscribe,
//         delete: async (space: SpaceConfig) => {
//             const persistedStore = await getPersistedStore();
//             await persistedStore.set(StoreKey.CurrentSpace, space);
//             await persistedStore.save();
//             set(space);

//             return;
//         },
//     };
// }

// export const spaces = createSpacesStore();
