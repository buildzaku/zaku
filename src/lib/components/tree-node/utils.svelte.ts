import { mount, unmount } from "svelte";
import { toast } from "svelte-sonner";

import { TreeNodePreview } from "$lib/components/tree-node";
import { sharedState, explorerActionsState, explorerState } from "$lib/state.svelte";
import type { DragOverDto, DragPayload, TreeNode } from "$lib/models";
import { RELATIVE_SPACE_ROOT } from "$lib/utils/constants";
import { commands } from "$lib/bindings";
import type { Collection, HttpReq } from "$lib/bindings";
import { emitCmdError } from "$lib/utils";

export function isCol(treeNode: TreeNode): treeNode is Collection {
    return Object.hasOwn(treeNode, "requests") && Object.hasOwn(treeNode, "collections");
}

export function isReq(treeNode: TreeNode): treeNode is HttpReq {
    return (
        Object.hasOwn(treeNode, "status") &&
        Object.hasOwn(treeNode, "config") &&
        Object.hasOwn(treeNode, "response")
    );
}

export function isSubPath(currentPath: string, targetPath: string): boolean {
    const currentSegments = pathSegments(currentPath);
    const targetPathSegments = pathSegments(targetPath);

    return (
        currentSegments.length <= targetPathSegments.length &&
        currentSegments.every((segment, index) => segment === targetPathSegments[index])
    );
}

export function pathSegments(path: string) {
    return path.split("/").filter(segment => segment !== "");
}

export function pathJoin(paths: string[]) {
    return paths.filter(segment => segment !== "").join("/");
}

export function isDropAllowed(path: string): boolean {
    if (explorerActionsState.dropTargetPath !== null && explorerActionsState.dragPayload !== null) {
        if (
            explorerActionsState.dropTargetPath ===
            explorerActionsState.dragPayload.parentRelativePath
        ) {
            return false;
        }

        if (isCol(explorerActionsState.dragPayload.node)) {
            const dirName = explorerActionsState.dragPayload.node.meta.fsname;
            const relativePath =
                explorerActionsState.dragPayload.parentRelativePath === RELATIVE_SPACE_ROOT
                    ? explorerActionsState.dragPayload.parentRelativePath.concat(dirName)
                    : explorerActionsState.dragPayload.parentRelativePath
                          .concat("/")
                          .concat(dirName);

            if (isSubPath(relativePath, explorerActionsState.dropTargetPath)) {
                return false;
            }

            return (
                explorerActionsState.dropTargetPath === path &&
                explorerActionsState.dropTargetPath !== relativePath
            );
        } else {
            return (
                explorerActionsState.dropTargetPath === path &&
                explorerActionsState.dropTargetPath !==
                    explorerActionsState.dragPayload.parentRelativePath
            );
        }
    }

    return false;
}

export function handleDragStart(event: DragEvent, payload: DragPayload) {
    event.stopImmediatePropagation();

    explorerActionsState.dragPayload = payload;

    if (event.dataTransfer) {
        const previewContainer = document.createElement("div");
        previewContainer.style.position = "absolute";
        previewContainer.style.top = "-1000px";
        previewContainer.style.left = "-1000px";
        document.body.appendChild(previewContainer);

        const previewTitle = isCol(payload.node)
            ? (payload.node.meta.name ?? payload.node.meta.fsname)
            : (payload.node.meta.name ?? payload.node.meta.fsname);

        const treeNodePreview = mount(TreeNodePreview, {
            target: previewContainer,
            props: { title: previewTitle },
        });

        if (previewContainer.firstElementChild instanceof HTMLElement) {
            const dragImage = previewContainer.firstElementChild;
            event.dataTransfer.setDragImage(dragImage, 0, 0);

            function cleanup() {
                unmount(treeNodePreview);
                document.body.removeChild(previewContainer);
            }

            if (event.currentTarget && event.currentTarget instanceof HTMLElement) {
                event.currentTarget.setAttribute("aria-grabbed", "true");
                event.currentTarget.addEventListener("dragend", cleanup, { once: true });
            }
        }
    }
}

export async function handleDrop(event: DragEvent) {
    event.preventDefault();
    event.stopImmediatePropagation();

    if (explorerActionsState.dragPayload === null) {
        toast.error("Drag payload not found");
        return;
    }
    if (explorerActionsState.dropTargetPath === null) {
        toast.error("Drop target path not found");
        return;
    }

    const from_relpath = buildPath(
        explorerActionsState.dragPayload.parentRelativePath,
        explorerActionsState.dragPayload.node.meta.fsname,
    );
    const to_relpath = buildPath(
        explorerActionsState.dropTargetPath,
        explorerActionsState.dragPayload.node.meta.fsname,
    );
    const node_type = isCol(explorerActionsState.dragPayload.node) ? "collection" : "request";

    const moveTreeNodeResult = await commands.moveTreeNode({
        node_type,
        from_relpath,
        to_relpath,
    });
    if (moveTreeNodeResult.status !== "ok") {
        return emitCmdError(moveTreeNodeResult.error);
    }

    await sharedState.synchronize();
}

export function handleDragOver(event: DragEvent, dragOverDto: DragOverDto) {
    event.preventDefault();
    event.stopImmediatePropagation();

    if (dragOverDto.type === "collection") {
        explorerActionsState.dropTargetPath = dragOverDto.relativePath;
    } else {
        explorerActionsState.dropTargetPath = dragOverDto.parentRelativePath;
    }
}

export function handleDragEnd(event: DragEvent) {
    event.stopImmediatePropagation();

    if (event.currentTarget instanceof HTMLElement) {
        event.currentTarget.setAttribute("aria-grabbed", "false");
    }

    explorerActionsState.dropTargetPath = null;
}

export function buildPath(currentPath: string, treeNodeName: string) {
    return currentPath === RELATIVE_SPACE_ROOT ? treeNodeName : `${currentPath}/${treeNodeName}`;
}

export function isCurrentCollectionOrAnyOfItsChildFocussed(currentPath: string): boolean {
    const isCurrentCollectionFocussed =
        explorerState.focussedNode.type === "collection" &&
        explorerState.focussedNode.relativePath === currentPath;
    const isCurrentCollectionChildFocussed =
        explorerState.focussedNode.type === "request" &&
        explorerState.focussedNode.parentRelativePath === currentPath;

    return isCurrentCollectionFocussed || isCurrentCollectionChildFocussed;
}
