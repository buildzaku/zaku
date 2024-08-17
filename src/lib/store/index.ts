import { writable } from "svelte/store";
import { Store } from "@tauri-apps/plugin-store";
import { appDataDir } from "@tauri-apps/api/path";
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

// export type SerializedKey<T extends string> = T | `_${T}`;
export type MaybeKey<T extends string = string> = T | `_${T}`;

export type SerializedValue = string | number | boolean | null;

export type HttpRequestConfig = {
    // id: string;
    // name: string;
    // description: string;
    // config: RequestConfig | null;

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

export async function getPersistedStore() {
    const dataPath = await appDataDir();

    return new Store(dataPath);
}

export const StoreKey = {
    CurrentWorkspacePath: "active_workspace_path",
    WorkspacePaths: "workspace_paths",
};

export type WorkspaceStoreDto = {
    path: string;
    name: string;
};

// Define the type for WorkspaceConfig
export type WorkspaceConfig = {
    name: string;
};

// Define the type for Request
export type Request = {
    name: string;
};

// Define the type for Collection
export type Collection = {
    name: string;
    requests: Request[];
};

// Define the type for Workspace
export type Workspace = {
    path: string;
    config: WorkspaceConfig;
    collections: Collection[];
    requests: Request[];
};

type CreateWorkspaceDto = {
    path: string;
    name: string;
};

function createWorkspaceStore() {
    const { set, subscribe } = writable<Workspace | null>(null);

    async function synchronize() {
        const activeWorkspace: Workspace | null = await invoke("get_active_workspace");
        set(activeWorkspace);
        await tick();

        return;
    }

    return {
        synchronize,
        set: async (path: string) => {
            console.log("invookdingng");
            await invoke("set_active_workspace", { path });
            await synchronize();

            return;
        },
        delete: async () => {
            await invoke("delete_active_workspace");
            await synchronize();

            return;
        },
        subscribe,
    };
}

export const activeWorkspace = createWorkspaceStore();

export async function createWorkspace(dto: CreateWorkspaceDto) {
    try {
        const createWorkspaceResultSchema = Struct.strictObject({
            path: Struct.string(),
        });
        const createWorkspaceRawResult = await invoke("create_workspace", {
            createWorkspaceDto: dto,
        });

        const createWorkspaceResult = Struct.parse(
            createWorkspaceResultSchema,
            createWorkspaceRawResult,
        );

        await activeWorkspace.set(createWorkspaceResult.path);

        return;
    } catch (error) {
        console.error("yoyoyoyoyoy createWorkspace");

        console.log(error);
    }
}

// function createWorkspacesStore() {
//     const { set, subscribe } = writable<Workspace[]>([]);

//     return {
//         set,
//         subscribe,
//         delete: async (workspace: WorkspaceConfig) => {
//             const persistedStore = await getPersistedStore();
//             await persistedStore.set(StoreKey.CurrentWorkspace, workspace);
//             await persistedStore.save();
//             set(workspace);

//             return;
//         },
//     };
// }

// export const workspaces = createWorkspacesStore();
