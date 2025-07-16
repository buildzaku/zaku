import { tick } from "svelte";
import { toast } from "svelte-sonner";

import { version } from "$app/environment";

import type { ActiveRequest, DragPayload, FocussedTreeNode } from "$lib/models";
import { commands } from "$lib/bindings";
import type { SpaceReference, Space, HttpReq } from "$lib/bindings";
import { joinPaths } from "$lib/components/tree-item/utils.svelte";

class SharedState {
    public activeSpace: Space | null = $state(null);
    public spaceRefs: SpaceReference[] = $state([]);

    public async synchronize() {
        const getSharedStateResult = await commands.getSharedState();

        if (getSharedStateResult.status === "ok") {
            this.activeSpace = getSharedStateResult.data.active_space;
            this.spaceRefs = getSharedStateResult.data.spacerefs;

            treeActionsState.reset();
            await tick();
        } else {
            console.error(getSharedStateResult.error);
            toast.error("Something went wrong while synchronizing state");
        }
    }

    public async setActiveSpace(spaceReference: SpaceReference) {
        const setActiveSpaceResult = await commands.setActiveSpace(spaceReference);

        if (setActiveSpaceResult.status === "ok") {
            await this.synchronize();
        } else {
            console.error(setActiveSpaceResult.error);
            toast.error("Something went wrong while setting space");
        }

        return;
    }
}

export const sharedState = new SharedState();

class TreeActionsState {
    public dragPayload: DragPayload | null = $state(null);
    public dropTargetPath: string | null = $state(null);
    public createNewNode: "collection" | "request" | null = $state(null);

    public reset() {
        this.dragPayload = null;
        this.dropTargetPath = null;
        this.createNewNode = null;
    }
}

export const treeActionsState = new TreeActionsState();

class TreeNodesState {
    #rootNode: FocussedTreeNode = {
        type: "collection",
        relativePath: "",
        parentRelativePath: "",
    };

    public focussedNode: FocussedTreeNode = $state(this.#rootNode);
    public activeRequest: ActiveRequest | null = $state(null);
    public openRequests: HttpReq[] = $state([]);

    public reset() {
        this.focussedNode = this.#rootNode;
        this.activeRequest = null;
        this.openRequests = [];
    }
}

export const treeNodesState = new TreeNodesState();

type AbsoluteRequestPath = string;

type DebouncedState = {
    timer: NodeJS.Timeout;
    absoluteSpacePath: string;
    activeRequest: ActiveRequest;
};

class Debounced {
    #state: Map<AbsoluteRequestPath, DebouncedState> = new Map();
    #DELAY = 1500;

    async #invokeSaveReqToBuf(absoluteSpacePath: string, activeRequest: ActiveRequest) {
        await commands.persistToReqbuf(
            absoluteSpacePath,
            activeRequest.parentRelativePath,
            activeRequest.self,
        );
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
            this.#invokeSaveReqToBuf(absoluteSpacePath, activeRequest);
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
            await this.#invokeSaveReqToBuf(absoluteSpacePath, activeRequest);
            this.#state.delete(absoluteRequestPath);
            clearTimeout(timer);
        }
    }
    public async flushAll(): Promise<void> {
        for (const { timer, absoluteSpacePath, activeRequest } of this.#state.values()) {
            await this.#invokeSaveReqToBuf(absoluteSpacePath, activeRequest);
            clearTimeout(timer);
        }
    }
}

export const debounced = new Debounced();

export const baseRequestHeaders: [boolean, string, string][] = $state([
    [true, "Cache-Control", "no-cache"],
    [true, "User-Agent", `Zaku/${version}`],
]);
