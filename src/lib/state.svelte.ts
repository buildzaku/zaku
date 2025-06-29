import { tick } from "svelte";
import { toast } from "svelte-sonner";

import { version } from "$app/environment";
import { RELATIVE_SPACE_ROOT, REQUEST_BODY_TYPES } from "$lib/utils/constants";
import { Err, Ok } from "$lib/utils";
import type { ValueOf } from "$lib/utils";
import { safeInvoke } from "$lib/commands";
import { TreeItemType } from "$lib/models";
import type { ActiveRequest, DragPayload, FocussedTreeItem } from "$lib/models";
import type { ZakuState as TZakuState, SpaceReference, Space, Request } from "$lib/bindings";
import { joinPaths } from "./components/tree-item/utils.svelte";

export type RequestConfig = ZakuRequestConfig;

export type MaybeKey<T extends string = string> = T | `_${T}`;

export type SerializedValue = string | number | boolean | null;

export type ZakuRequestConfig = {
    name: string;
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
    public dragPayload: DragPayload | null = $state(null);
    public dropTargetPath: string | null = $state(null);
    public createNewItem: TreeItemType | null = $state(null);

    public reset() {
        this.dragPayload = null;
        this.dropTargetPath = null;
        this.createNewItem = null;
    }
}

export const treeActionsState = new TreeActionsState();

class TreeItemsState {
    #rootItem: FocussedTreeItem = {
        type: TreeItemType.Collection,
        relativePath: RELATIVE_SPACE_ROOT,
        parentRelativePath: RELATIVE_SPACE_ROOT,
    };

    public focussedItem: FocussedTreeItem = $state(this.#rootItem);
    public activeRequest: ActiveRequest | null = $state(null);
    public openRequests: Request[] = $state([]);

    public reset() {
        this.focussedItem = this.#rootItem;
        this.activeRequest = null;
        this.openRequests = [];
    }
}

export const treeItemsState = new TreeItemsState();

type AbsoluteRequestPath = string;

type DebouncedState = {
    timer: NodeJS.Timeout;
    absoluteSpacePath: string;
    activeRequest: ActiveRequest;
};

class Debounced {
    #state: Map<AbsoluteRequestPath, DebouncedState> = new Map();
    #DELAY = 1500;

    async #invokeSaveRequestToBuffer(absoluteSpacePath: string, activeRequest: ActiveRequest) {
        await safeInvoke("save_request_to_buffer", {
            absolute_space_path: absoluteSpacePath,
            relative_path: activeRequest.parentRelativePath,
            request: activeRequest.self,
        });
    }
    public saveRequestToBuffer(absoluteSpacePath: string, activeRequest: ActiveRequest): void {
        const absoluteRequestPath = joinPaths([
            absoluteSpacePath,
            activeRequest.parentRelativePath,
            activeRequest.self.meta.file_name,
        ]);

        const current = this.#state.get(absoluteRequestPath);
        if (current) {
            clearTimeout(current.timer);
        }

        const timer = setTimeout(() => {
            this.#invokeSaveRequestToBuffer(absoluteSpacePath, activeRequest);
            this.#state.delete(absoluteRequestPath);
        }, this.#DELAY);

        this.#state.set(absoluteRequestPath, {
            timer,
            absoluteSpacePath,
            activeRequest,
        });
    }
    public isPending(absoluteRequestPath: string): boolean {
        return this.#state.has(absoluteRequestPath);
    }
    public async flush(absoluteRequestPath: string): Promise<void> {
        const currentState = this.#state.get(absoluteRequestPath);
        if (currentState) {
            const { timer, absoluteSpacePath, activeRequest } = currentState;
            await this.#invokeSaveRequestToBuffer(absoluteSpacePath, activeRequest);
            this.#state.delete(absoluteRequestPath);
            clearTimeout(timer);
        }
    }
    public async flushAll(): Promise<void> {
        for (const { timer, absoluteSpacePath, activeRequest } of this.#state.values()) {
            await this.#invokeSaveRequestToBuffer(absoluteSpacePath, activeRequest);
            clearTimeout(timer);
        }
    }
}

export const debounced = new Debounced();

export const baseRequestHeaders: [boolean, string, string][] = $state([
    [true, "Cache-Control", "no-cache"],
    [true, "User-Agent", `Zaku/${version}`],
]);
