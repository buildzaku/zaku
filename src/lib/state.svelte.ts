import { tick } from "svelte";

import { version } from "$app/environment";

import type { OpenRequest, FocussedTreeNode, TreeNode } from "$lib/models";
import { commands } from "$lib/bindings";
import type { SpaceReference, Space, HttpReq, NodeType } from "$lib/bindings";
import { Path } from "$lib/utils/path";
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
    public dragNode: TreeNode | null = $state(null);
    public dragNodePath: Path | null = $state(null);
    public dropNodePath: Path | null = $state(null);
    public createNewNode: NodeType | null = $state(null);

    public reset() {
        this.dragNode = null;
        this.dragNodePath = null;
        this.dropNodePath = null;
        this.createNewNode = null;
    }
}

export const explorerActionsState = new ExplorerActionsState();

class ExplorerState {
    #rootNode: FocussedTreeNode = {
        type: "collection",
        relpath: Path.from(""),
    };

    public focussedNode: FocussedTreeNode = $state(this.#rootNode);
    public openRequest: OpenRequest | null = $state(null);
    public backgroundRequests: HttpReq[] = $state([]);

    public setFocussedNode(focussedNode: FocussedTreeNode) {
        if (this.focussedNode.relpath.toString() !== focussedNode.relpath.toString()) {
            this.focussedNode = focussedNode;
        }
    }

    public setOpenRequest(openRequest: OpenRequest) {
        const currentRelpath = this.openRequest ? this.openRequest.self.meta.relpath : null;
        const newRelpath = openRequest.self.meta.relpath;

        if (currentRelpath !== newRelpath) {
            this.openRequest = openRequest;

            if (!this.backgroundRequests.includes(openRequest.self)) {
                this.backgroundRequests.push(openRequest.self);
            }
        }
    }

    public isCreateNewNodeParent(relpath: Path): boolean {
        const isCurCollectionFocussed =
            this.focussedNode.type === "collection" &&
            this.focussedNode.relpath.toString() === relpath.toString();
        const isCurCollectionReqFocussed =
            this.focussedNode.type === "request" &&
            this.focussedNode.relpath.parent()?.toString() === relpath.toString();

        return isCurCollectionFocussed || isCurCollectionReqFocussed;
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
    spaceAbspath: string;
    openRequest: OpenRequest;
};

class Debounced {
    #state: Map<AbsoluteRequestPath, DebouncedState> = new Map();
    #DELAY = 1500;

    public saveReqToSpaceBuffer(spaceAbspath: string, openRequest: OpenRequest): void {
        const reqAbspath = Path.from(spaceAbspath).join(openRequest.self.meta.relpath).toString();

        const current = this.#state.get(reqAbspath);
        if (current) {
            clearTimeout(current.timer);
        }

        const timer = setTimeout(() => {
            commands.writeReqToSpaceBuffer(spaceAbspath, openRequest.self);

            this.#state.delete(reqAbspath);
        }, this.#DELAY);

        this.#state.set(reqAbspath, {
            timer,
            spaceAbspath,
            openRequest,
        });
    }
    public isPending(reqAbspath: string): boolean {
        return this.#state.has(reqAbspath);
    }
    public async flush(reqAbspath: string): Promise<void> {
        const currentState = this.#state.get(reqAbspath);
        if (currentState) {
            const { timer, spaceAbspath, openRequest } = currentState;
            await commands.writeReqToSpaceBuffer(spaceAbspath, openRequest.self);
            this.#state.delete(reqAbspath);
            clearTimeout(timer);
        }
    }
    public async flushAll(): Promise<void> {
        for (const { timer, spaceAbspath, openRequest } of this.#state.values()) {
            commands.writeReqToSpaceBuffer(spaceAbspath, openRequest.self);
            clearTimeout(timer);
        }
    }
}

export const debounced = new Debounced();

export const baseRequestHeaders: [boolean, string, string][] = $state([
    [true, "Cache-Control", "no-cache"],
    [true, "User-Agent", `Zaku/${version}`],
]);
