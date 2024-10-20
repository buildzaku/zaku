import { get } from "svelte/store";
import { toast } from "svelte-sonner";

import TreeItemPreview from "./tree-item-preview.svelte";
import { currentDragPayload, currentDropTargetPath, focussedTreeItem, zakuState } from "$lib/store";
import { TREE_ITEM_TYPE } from "$lib/models";
import type { DragOverDto, DragPayload, RemoveTreeItemDto } from "$lib/models";
import { RELATIVE_SPACE_ROOT } from "$lib/utils/constants";
import {
    addTreeItemToCollection,
    isCollection,
    isSubPath,
    removeTreeItemFromCollection,
} from "$lib/utils/tree";
import { safeInvoke } from "$lib/commands";
import type { MoveTreeItemDto } from "$lib/bindings";

export function isDropAllowed(path: string): boolean {
    const staticCurrentDropTargetPath = get(currentDropTargetPath);
    const staticCurrentDragPayload = get(currentDragPayload);

    if (staticCurrentDropTargetPath !== null && staticCurrentDragPayload !== null) {
        if (staticCurrentDropTargetPath === staticCurrentDragPayload.parentRelativePath) {
            return false;
        }

        if (isCollection(staticCurrentDragPayload.treeItem)) {
            const dirName = staticCurrentDragPayload.treeItem.meta.dir_name;
            const relativePath =
                staticCurrentDragPayload.parentRelativePath === RELATIVE_SPACE_ROOT
                    ? staticCurrentDragPayload.parentRelativePath.concat(dirName)
                    : staticCurrentDragPayload.parentRelativePath.concat("/").concat(dirName);

            if (isSubPath(relativePath, staticCurrentDropTargetPath)) {
                return false;
            }

            return (
                staticCurrentDropTargetPath === path && staticCurrentDropTargetPath !== relativePath
            );
        } else {
            return (
                staticCurrentDropTargetPath === path &&
                staticCurrentDropTargetPath !== staticCurrentDragPayload.parentRelativePath
            );
        }
    }

    return false;
}

export function handleDragStart(event: DragEvent, payload: DragPayload) {
    event.stopImmediatePropagation();
    currentDragPayload.set(payload);

    if (event.dataTransfer) {
        const previewContainer = document.createElement("div");
        previewContainer.style.position = "absolute";
        previewContainer.style.top = "-1000px";
        previewContainer.style.left = "-1000px";
        document.body.appendChild(previewContainer);

        const previewTitle = isCollection(payload.treeItem)
            ? (payload.treeItem.meta.display_name ?? payload.treeItem.meta.dir_name)
            : (payload.treeItem.meta.display_name ?? payload.treeItem.meta.file_name);

        const previewInstance = new TreeItemPreview({
            target: previewContainer,
            props: { title: previewTitle },
        });

        if (previewContainer.firstElementChild instanceof HTMLElement) {
            const dragImage = previewContainer.firstElementChild;
            event.dataTransfer.setDragImage(dragImage, 0, 0);

            function cleanup() {
                previewInstance.$destroy();
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

    const staticZakuState = get(zakuState);
    const staticCurrentDragPayload = get(currentDragPayload);
    const staticCurrentDropTargetPath = get(currentDropTargetPath);

    if (staticZakuState.active_space === null) {
        console.warn("Active space not found");
        return;
    }
    if (staticCurrentDragPayload === null) {
        console.warn("Drag payload not found");
        return;
    }
    if (staticCurrentDropTargetPath === null) {
        console.warn("Drop target path not found");
        return;
    }

    const mutRootCollection = staticZakuState.active_space.root;
    const addTreeItemToCollectionResult = addTreeItemToCollection({
        parentRelativePath: staticCurrentDragPayload.parentRelativePath,
        treeItem: staticCurrentDragPayload.treeItem,
        targetPath: staticCurrentDropTargetPath,
        mutRootCollection,
    });
    if (!addTreeItemToCollectionResult.ok) {
        console.error("Cannot add tree item to the collection");
        return;
    }

    const removeTreeItemDto: RemoveTreeItemDto = isCollection(staticCurrentDragPayload.treeItem)
        ? { type: "collection", dir_name: staticCurrentDragPayload.treeItem.meta.dir_name }
        : { type: "request", file_name: staticCurrentDragPayload.treeItem.meta.file_name };
    const removeTreeItemFromCollectionResult = removeTreeItemFromCollection({
        parentRelativePath: staticCurrentDragPayload.parentRelativePath,
        removeTreeItemDto,
        mutRootCollection,
    });
    if (!removeTreeItemFromCollectionResult.ok) {
        console.error("Unable to remove tree item from the collection");
        return;
    }

    const fileOrDirName = isCollection(staticCurrentDragPayload.treeItem)
        ? staticCurrentDragPayload.treeItem.meta.dir_name
        : staticCurrentDragPayload.treeItem.meta.file_name;
    const moveTreeItemDto: MoveTreeItemDto = {
        source_relative_path: buildPath(staticCurrentDragPayload.parentRelativePath, fileOrDirName),
        destination_relative_path: buildPath(staticCurrentDropTargetPath, fileOrDirName),
    };
    const moveTreeItemResult = await safeInvoke("move_tree_item", {
        move_tree_item_dto: moveTreeItemDto,
    });
    if (!moveTreeItemResult.ok) {
        console.error(moveTreeItemResult.err);
        toast(
            `Something went wrong. Unable to move \`${staticCurrentDragPayload.treeItem.meta.display_name}\``,
        );

        return;
    }

    zakuState.update(state => {
        if (state.active_space === null) {
            return state;
        }

        return {
            ...state,
            active_space: {
                ...state.active_space,
                root: mutRootCollection,
            },
        };
    });

    currentDragPayload.set(null);
    currentDropTargetPath.set(null);
}

export function handleDragOver(event: DragEvent, dragOverDto: DragOverDto) {
    event.preventDefault();
    event.stopImmediatePropagation();

    if (dragOverDto.type === "collection") {
        currentDropTargetPath.set(dragOverDto.relativePath);
    } else {
        currentDropTargetPath.set(dragOverDto.parentRelativePath);
    }
}

export function handleDragEnd(event: DragEvent) {
    event.stopImmediatePropagation();

    if (event.currentTarget instanceof HTMLElement) {
        event.currentTarget.setAttribute("aria-grabbed", "false");
    }

    currentDropTargetPath.set(null);
}

export function buildPath(currentPath: string, treeItemName: string) {
    return currentPath === RELATIVE_SPACE_ROOT ? treeItemName : `${currentPath}/${treeItemName}`;
}

export function isCurrentCollectionOrAnyOfItsChildFocussed(currentPath: string): boolean {
    const staticFocussedTreeItem = get(focussedTreeItem);
    const isCurrentCollectionFocussed =
        staticFocussedTreeItem.type === TREE_ITEM_TYPE.Collection &&
        staticFocussedTreeItem.relativePath === currentPath;
    const isCurrentCollectionChildFocussed =
        staticFocussedTreeItem.type === TREE_ITEM_TYPE.Request &&
        staticFocussedTreeItem.parentRelativePath === currentPath;

    return isCurrentCollectionFocussed || isCurrentCollectionChildFocussed;
}

export { default as TreeItemContent } from "./tree-item-content.svelte";
export { default as TreeItemCreate } from "./tree-item-create.svelte";
export { default as TreeItemPreview } from "./tree-item-preview.svelte";
export { default as TreeItemRoot } from "./tree-item-root.svelte";
