import { mount, unmount } from "svelte";
import { toast } from "svelte-sonner";

import { TreeItemPreview } from "$lib/components/tree-item";
import { zakuState, treeActionsState, treeItemsState } from "$lib/state.svelte";
import { TreeItemType } from "$lib/models";
import type { DragOverDto, DragPayload, RemoveTreeItemDto, TreeItem } from "$lib/models";
import { RELATIVE_SPACE_ROOT } from "$lib/utils/constants";
import { commands, type Collection, type MoveTreeItemDto } from "$lib/bindings";
import { Err, Ok } from "$lib/utils";
import type { Result } from "$lib/utils";

export function isCollection(treeItem: TreeItem): treeItem is Collection {
    return Object.hasOwn(treeItem.meta, "dir_name");
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

export function joinPaths(paths: string[]) {
    return paths.filter(segment => segment !== "").join("/");
}

export function isDropAllowed(path: string): boolean {
    if (treeActionsState.dropTargetPath !== null && treeActionsState.dragPayload !== null) {
        if (treeActionsState.dropTargetPath === treeActionsState.dragPayload.parentRelativePath) {
            return false;
        }

        if (isCollection(treeActionsState.dragPayload.treeItem)) {
            const dirName = treeActionsState.dragPayload.treeItem.meta.dir_name;
            const relativePath =
                treeActionsState.dragPayload.parentRelativePath === RELATIVE_SPACE_ROOT
                    ? treeActionsState.dragPayload.parentRelativePath.concat(dirName)
                    : treeActionsState.dragPayload.parentRelativePath.concat("/").concat(dirName);

            if (isSubPath(relativePath, treeActionsState.dropTargetPath)) {
                return false;
            }

            return (
                treeActionsState.dropTargetPath === path &&
                treeActionsState.dropTargetPath !== relativePath
            );
        } else {
            return (
                treeActionsState.dropTargetPath === path &&
                treeActionsState.dropTargetPath !== treeActionsState.dragPayload.parentRelativePath
            );
        }
    }

    return false;
}

export function handleDragStart(event: DragEvent, payload: DragPayload) {
    event.stopImmediatePropagation();

    treeActionsState.dragPayload = payload;

    if (event.dataTransfer) {
        const previewContainer = document.createElement("div");
        previewContainer.style.position = "absolute";
        previewContainer.style.top = "-1000px";
        previewContainer.style.left = "-1000px";
        document.body.appendChild(previewContainer);

        const previewTitle = isCollection(payload.treeItem)
            ? (payload.treeItem.meta.display_name ?? payload.treeItem.meta.dir_name)
            : (payload.treeItem.meta.display_name ?? payload.treeItem.meta.file_name);

        const treeItemPreview = mount(TreeItemPreview, {
            target: previewContainer,
            props: { title: previewTitle },
        });

        if (previewContainer.firstElementChild instanceof HTMLElement) {
            const dragImage = previewContainer.firstElementChild;
            event.dataTransfer.setDragImage(dragImage, 0, 0);

            function cleanup() {
                unmount(treeItemPreview);
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

    if (zakuState.activeSpace === null) {
        console.warn("Active space not found");
        return;
    }
    if (treeActionsState.dragPayload === null) {
        console.warn("Drag payload not found");
        return;
    }
    if (treeActionsState.dropTargetPath === null) {
        console.warn("Drop target path not found");
        return;
    }

    const mutRootCollection = zakuState.activeSpace.root;
    const addTreeItemToCollectionResult = addTreeItemToCollection({
        parentRelativePath: treeActionsState.dragPayload.parentRelativePath,
        treeItem: treeActionsState.dragPayload.treeItem,
        targetPath: treeActionsState.dropTargetPath,
        mutRootCollection,
    });
    if (!addTreeItemToCollectionResult.ok) {
        console.error("Cannot add tree item to the collection");
        return;
    }

    const removeTreeItemDto: RemoveTreeItemDto = isCollection(treeActionsState.dragPayload.treeItem)
        ? {
              type: TreeItemType.Collection,
              dir_name: treeActionsState.dragPayload.treeItem.meta.dir_name,
          }
        : {
              type: TreeItemType.Request,
              file_name: treeActionsState.dragPayload.treeItem.meta.file_name,
          };
    const removeTreeItemFromCollectionResult = removeTreeItemFromCollection({
        parentRelativePath: treeActionsState.dragPayload.parentRelativePath,
        removeTreeItemDto,
        mutRootCollection,
    });
    if (!removeTreeItemFromCollectionResult.ok) {
        console.error("Unable to remove tree item from the collection");
        return;
    }

    const fileOrDirName = isCollection(treeActionsState.dragPayload.treeItem)
        ? treeActionsState.dragPayload.treeItem.meta.dir_name
        : treeActionsState.dragPayload.treeItem.meta.file_name;
    const moveTreeItemDto: MoveTreeItemDto = {
        source_relative_path: buildPath(
            treeActionsState.dragPayload.parentRelativePath,
            fileOrDirName,
        ),
        destination_relative_path: buildPath(treeActionsState.dropTargetPath, fileOrDirName),
    };
    const moveTreeItemResult = await commands.moveTreeItem(moveTreeItemDto);
    if (moveTreeItemResult.status === "error") {
        console.error(moveTreeItemResult.error);
        toast(
            `Something went wrong. Unable to move \`${treeActionsState.dragPayload.treeItem.meta.display_name}\``,
        );

        return;
    }

    treeActionsState.dragPayload = null;
    treeActionsState.dropTargetPath = null;
}

export function handleDragOver(event: DragEvent, dragOverDto: DragOverDto) {
    event.preventDefault();
    event.stopImmediatePropagation();

    if (dragOverDto.type === "collection") {
        treeActionsState.dropTargetPath = dragOverDto.relativePath;
    } else {
        treeActionsState.dropTargetPath = dragOverDto.parentRelativePath;
    }
}

export function handleDragEnd(event: DragEvent) {
    event.stopImmediatePropagation();

    if (event.currentTarget instanceof HTMLElement) {
        event.currentTarget.setAttribute("aria-grabbed", "false");
    }

    treeActionsState.dropTargetPath = null;
}

export function buildPath(currentPath: string, treeItemName: string) {
    return currentPath === RELATIVE_SPACE_ROOT ? treeItemName : `${currentPath}/${treeItemName}`;
}

export function isCurrentCollectionOrAnyOfItsChildFocussed(currentPath: string): boolean {
    const isCurrentCollectionFocussed =
        treeItemsState.focussedItem.type === "collection" &&
        treeItemsState.focussedItem.relativePath === currentPath;
    const isCurrentCollectionChildFocussed =
        treeItemsState.focussedItem.type === "request" &&
        treeItemsState.focussedItem.parentRelativePath === currentPath;

    return isCurrentCollectionFocussed || isCurrentCollectionChildFocussed;
}

export type AddTreeItemToCollectionParams = {
    parentRelativePath: string;
    treeItem: TreeItem;
    targetPath: string;
    mutRootCollection: Collection;
};

export function addTreeItemToCollection({
    parentRelativePath,
    treeItem,
    targetPath,
    mutRootCollection,
}: AddTreeItemToCollectionParams): Result<void> {
    if (targetPath === parentRelativePath) {
        console.warn(`Abort dropping to the same parent \`${parentRelativePath}\``);
        return Err();
    }

    let current: Collection = mutRootCollection;
    let traversedPath = "/";

    for (const segment of pathSegments(targetPath)) {
        const nextCollection = current.collections.find(
            collection => collection.meta.dir_name === segment,
        );
        if (!nextCollection) {
            console.warn(`Target collection \`${segment}\` not found in \`${traversedPath}\``);
            return Err();
        }

        current = nextCollection;
        traversedPath =
            traversedPath === "/"
                ? traversedPath.concat(segment)
                : traversedPath.concat("/").concat(segment);
    }

    if (isCollection(treeItem)) {
        const relativePath =
            parentRelativePath === RELATIVE_SPACE_ROOT
                ? parentRelativePath.concat(treeItem.meta.dir_name)
                : parentRelativePath.concat("/").concat(treeItem.meta.dir_name);

        if (isSubPath(relativePath, targetPath)) {
            console.warn(
                `Abort moving collection to itself or it's own child collection \`${targetPath}\``,
            );
            return Err();
        }
    } else {
        if (parentRelativePath === traversedPath) {
            console.warn(`Abort moving request into the same collection \`${targetPath}\``);
            return Err();
        }
    }

    if (isCollection(treeItem)) {
        const collectionDirNameAlreadyExists = current.collections.some(
            collection => collection.meta.dir_name === treeItem.meta.dir_name,
        );
        if (collectionDirNameAlreadyExists) {
            toast(
                `Collection with directory name ${treeItem.meta.dir_name} already exists in the ${current.meta.dir_name} collection`,
            );

            return Err();
        }
        current.collections.push(treeItem);
        current.collections.sort((a, b) =>
            a.meta.dir_name.toLocaleLowerCase().localeCompare(b.meta.dir_name),
        );

        return Ok();
    } else {
        const reqFileNameAlreadyExists = current.requests.some(
            request => request.meta.file_name === treeItem.meta.file_name,
        );
        if (reqFileNameAlreadyExists) {
            toast(
                `Request with file name ${treeItem.meta.file_name} already exists in the ${current.meta.dir_name} collection`,
            );

            return Err();
        }
        current.requests.push(treeItem);
        current.requests.sort((a, b) => a.meta.file_name.localeCompare(b.meta.file_name));

        return Ok();
    }
}

export type RemoveTreeItemFromCollectionParams = {
    parentRelativePath: string;
    removeTreeItemDto: RemoveTreeItemDto;
    mutRootCollection: Collection;
};

export function removeTreeItemFromCollection({
    parentRelativePath,
    removeTreeItemDto,
    mutRootCollection,
}: RemoveTreeItemFromCollectionParams): Result<void> {
    const segments = pathSegments(parentRelativePath);
    let current: Collection = mutRootCollection;

    for (const segment of segments) {
        const nextCollection = current.collections.find(
            collection => collection.meta.dir_name === segment,
        );
        if (!nextCollection) {
            console.warn(`Collection not found for segment: ${segment}`);

            return Err();
        }

        current = nextCollection;
    }

    if (removeTreeItemDto.type === "collection") {
        current.collections = current.collections.filter(
            collection => collection.meta.dir_name !== removeTreeItemDto.dir_name,
        );

        return Ok();
    } else {
        current.requests = current.requests.filter(
            request => request.meta.file_name !== removeTreeItemDto.file_name,
        );

        return Ok();
    }
}
