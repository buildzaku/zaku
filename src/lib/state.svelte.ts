import { tick } from "svelte";

import { version } from "$app/environment";

import type { OpenRequest, DragPayload, FocussedTreeNode } from "$lib/models";
import { commands } from "$lib/bindings";
import type { SpaceReference, Space, HttpReq, NodeType } from "$lib/bindings";
import { pathJoin } from "$lib/components/tree-node/utils.svelte";
import { emitCmdError } from "$lib/utils";

class SharedState {
    public space: Space | null = $state(null);
    public spaceRefs: SpaceReference[] = $state([]);

    public async synchronize() {
        const getSharedStateResult = await commands.getSharedState();
        if (getSharedStateResult.status !== "ok") {
            return emitCmdError(getSharedStateResult.error);
        }

        this.space = getSharedStateResult.data.space;
        this.spaceRefs = getSharedStateResult.data.spacerefs;

        explorerActionsState.reset();
        await tick();
    }

    public async setSpace(spaceReference: SpaceReference) {
        const setSpaceResult = await commands.setSpace(spaceReference);
        if (setSpaceResult.status !== "ok") {
            return emitCmdError(setSpaceResult.error);
        }

        await this.synchronize();
    }
}

export const sharedState = new SharedState();

class ExplorerActionsState {
    public dragPayload: DragPayload | null = $state(null);
    public dropTargetPath: string | null = $state(null);
    public createNewNode: NodeType | null = $state(null);

    public reset() {
        this.dragPayload = null;
        this.dropTargetPath = null;
        this.createNewNode = null;
    }
}

export const explorerActionsState = new ExplorerActionsState();

class ExplorerState {
    #rootNode: FocussedTreeNode = {
        type: "collection",
        relativePath: "",
        parentRelativePath: "",
    };

    public focussedNode: FocussedTreeNode = $state(this.#rootNode);
    public openRequest: OpenRequest | null = $state(null);
    public backgroundRequests: HttpReq[] = $state([]);

    public setFocussedNode(node: FocussedTreeNode) {
        if (this.focussedNode.relativePath !== node.relativePath) {
            this.focussedNode = node;
        }
    }

    public setOpenRequest(openRequest: OpenRequest) {
        const currentRelpath = this.openRequest
            ? pathJoin([this.openRequest.parentRelpath, this.openRequest.self.meta.fsname])
            : null;
        const newRelpath = pathJoin([openRequest.parentRelpath, openRequest.self.meta.fsname]);

        if (currentRelpath !== newRelpath) {
            this.openRequest = openRequest;

            if (!this.backgroundRequests.includes(openRequest.self)) {
                this.backgroundRequests.push(openRequest.self);
            }
        }
    }

    public reset() {
        this.focussedNode = this.#rootNode;
        this.openRequest = null;
        this.backgroundRequests = [];
    }
}

export const explorerState = new ExplorerState();

type AbsoluteRequestPath = string;

type DebouncedState = {
    timer: number;
    absoluteSpacePath: string;
    openRequest: OpenRequest;
};

class Debounced {
    #state: Map<AbsoluteRequestPath, DebouncedState> = new Map();
    #DELAY = 1500;

    public saveReqToSpaceBuffer(absoluteSpacePath: string, openRequest: OpenRequest): void {
        const absoluteRequestPath = pathJoin([
            absoluteSpacePath,
            openRequest.parentRelpath,
            openRequest.self.meta.fsname,
        ]);

        const current = this.#state.get(absoluteRequestPath);
        if (current) {
            clearTimeout(current.timer);
        }

        const timer = setTimeout(() => {
            commands.writeReqToSpaceBuffer(
                absoluteSpacePath,
                openRequest.parentRelpath,
                openRequest.self,
            );

            this.#state.delete(absoluteRequestPath);
        }, this.#DELAY);

        this.#state.set(absoluteRequestPath, {
            timer,
            absoluteSpacePath,
            openRequest,
        });
    }
    public isPending(absoluteRequestPath: string): boolean {
        return this.#state.has(absoluteRequestPath);
    }
    public async flush(absoluteRequestPath: string): Promise<void> {
        const currentState = this.#state.get(absoluteRequestPath);
        if (currentState) {
            const { timer, absoluteSpacePath, openRequest } = currentState;
            await commands.writeReqToSpaceBuffer(
                absoluteSpacePath,
                openRequest.parentRelpath,
                openRequest.self,
            );
            this.#state.delete(absoluteRequestPath);
            clearTimeout(timer);
        }
    }
    public async flushAll(): Promise<void> {
        for (const { timer, absoluteSpacePath, openRequest } of this.#state.values()) {
            commands.writeReqToSpaceBuffer(
                absoluteSpacePath,
                openRequest.parentRelpath,
                openRequest.self,
            );
            clearTimeout(timer);
        }
    }
}

export const debounced = new Debounced();

export const baseRequestHeaders: [boolean, string, string][] = $state([
    [true, "Cache-Control", "no-cache"],
    [true, "User-Agent", `Zaku/${version}`],
]);
