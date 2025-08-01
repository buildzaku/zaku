import { mount, unmount } from "svelte";
import { toast } from "svelte-sonner";

import { TreeNodePreview } from "$lib/components/tree-node";
import { sharedState, explorerActionsState } from "$lib/state.svelte";
import type { DragOverDto, TreeNode } from "$lib/models";
import { Path } from "$lib/utils/path";
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

export function isDropAllowed(path: string): boolean {
    if (
        explorerActionsState.dropNodePath !== null &&
        explorerActionsState.dragNode !== null &&
        explorerActionsState.dragNodePath !== null
    ) {
        const dragParentPath = explorerActionsState.dragNodePath.parent() ?? Path.from("");

        if (explorerActionsState.dropNodePath.toString() === dragParentPath.toString()) {
            return false;
        }

        if (isCol(explorerActionsState.dragNode)) {
            if (explorerActionsState.dropNodePath.startsWith(explorerActionsState.dragNodePath)) {
                return false;
            }

            return (
                explorerActionsState.dropNodePath.toString() === path &&
                explorerActionsState.dropNodePath.toString() !==
                    explorerActionsState.dragNodePath.toString()
            );
        } else {
            return (
                explorerActionsState.dropNodePath.toString() === path &&
                explorerActionsState.dropNodePath.toString() !== dragParentPath.toString()
            );
        }
    }

    return false;
}

export function handleDragStart(event: DragEvent, node: TreeNode) {
    event.stopImmediatePropagation();

    explorerActionsState.dragNode = node;
    explorerActionsState.dragNodePath = Path.from(node.meta.relpath);

    if (event.dataTransfer) {
        const previewContainer = document.createElement("div");
        previewContainer.style.position = "absolute";
        previewContainer.style.top = "-1000px";
        previewContainer.style.left = "-1000px";
        document.body.appendChild(previewContainer);

        const previewTitle = isCol(node)
            ? (node.meta.name ?? node.meta.fsname)
            : (node.meta.name ?? node.meta.fsname);

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

    if (explorerActionsState.dragNode === null) {
        toast.error("Drag node not found");
        return;
    }
    if (explorerActionsState.dragNodePath === null) {
        toast.error("Drag node path not found");
        return;
    }
    if (explorerActionsState.dropNodePath === null) {
        toast.error("Drop target path not found");
        return;
    }

    const moveTreeNodeResult = await commands.moveTreeNode({
        node_type: isCol(explorerActionsState.dragNode) ? "collection" : "request",
        cur_relpath: explorerActionsState.dragNodePath.toString(),
        nxt_relpath: explorerActionsState.dropNodePath
            .join(explorerActionsState.dragNode.meta.fsname)
            .toString(),
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
        explorerActionsState.dropNodePath = dragOverDto.relpath;
    } else {
        explorerActionsState.dropNodePath = dragOverDto.relpath.parent() ?? Path.from("");
    }
}

export function handleDragEnd(event: DragEvent) {
    event.stopImmediatePropagation();

    if (event.currentTarget instanceof HTMLElement) {
        event.currentTarget.setAttribute("aria-grabbed", "false");
    }

    explorerActionsState.dropNodePath = null;
}
