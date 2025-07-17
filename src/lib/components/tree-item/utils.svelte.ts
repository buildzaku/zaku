import { mount, unmount } from "svelte";
import { toast } from "svelte-sonner";

import { TreeNodePreview } from "$lib/components/tree-item";
import { sharedState, treeActionsState, treeNodesState } from "$lib/state.svelte";
import type { DragOverDto, DragPayload, TreeNode } from "$lib/models";
import { RELATIVE_SPACE_ROOT } from "$lib/utils/constants";
import { commands } from "$lib/bindings";
import type { Collection, HttpReq } from "$lib/bindings";

// TODO - add test
export function isCol(treeNode: TreeNode): treeNode is Collection {
    return Object.hasOwn(treeNode, "requests") && Object.hasOwn(treeNode, "collections");
}

// TODO - add test
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

export function joinPaths(paths: string[]) {
    return paths.filter(segment => segment !== "").join("/");
}

export function isDropAllowed(path: string): boolean {
    if (treeActionsState.dropTargetPath !== null && treeActionsState.dragPayload !== null) {
        if (treeActionsState.dropTargetPath === treeActionsState.dragPayload.parentRelativePath) {
            return false;
        }

        if (isCol(treeActionsState.dragPayload.node)) {
            const dirName = treeActionsState.dragPayload.node.meta.dir_name;
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

        const previewTitle = isCol(payload.node)
            ? (payload.node.meta.name ?? payload.node.meta.dir_name)
            : (payload.node.meta.name ?? payload.node.meta.file_name);

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

    if (treeActionsState.dragPayload === null) {
        toast.error("Drag payload not found");
        return;
    }
    if (treeActionsState.dropTargetPath === null) {
        toast.error("Drop target path not found");
        return;
    }

    const fileOrDirName = isCol(treeActionsState.dragPayload.node)
        ? treeActionsState.dragPayload.node.meta.dir_name
        : treeActionsState.dragPayload.node.meta.file_name;
    const src_relpath = buildPath(treeActionsState.dragPayload.parentRelativePath, fileOrDirName);
    const dest_relpath = buildPath(treeActionsState.dropTargetPath, fileOrDirName); // ← Add filename here

    console.log({ src_relpath, dest_relpath });

    const treeNodeDropResult = await commands.handleTreeNodeDrop({
        src_relpath,
        dest_relpath,
    });
    if (treeNodeDropResult.status === "error") {
        console.error(JSON.stringify(treeNodeDropResult.error, null, 2));
    }

    await sharedState.synchronize();
}

// export async function handleDrop(event: DragEvent) {
//     event.preventDefault();
//     event.stopImmediatePropagation();

//     if (sharedState.activeSpace === null) {
//         console.warn("Active space not found");
//         return;
//     }
//     if (treeActionsState.dragPayload === null) {
//         console.warn("Drag payload not found");
//         return;
//     }
//     if (treeActionsState.dropTargetPath === null) {
//         console.warn("Drop target path not found");
//         return;
//     }

//     const mutRootCollection = sharedState.activeSpace.root_collection;
//     const addTreeNodeToCollectionResult = addTreeNodeToCollection({
//         parentRelativePath: treeActionsState.dragPayload.parentRelativePath,
//         treeNode: treeActionsState.dragPayload.node,
//         targetPath: treeActionsState.dropTargetPath,
//         mutRootCollection,
//     });
//     if (addTreeNodeToCollectionResult.status === "error") {
//         console.error("Cannot add tree item to the collection");
//         return;
//     }

//     const removeTreeNodeDto: RemoveTreeNodeDto = isCol(treeActionsState.dragPayload.node)
//         ? {
//               type: "collection",
//               dir_name: treeActionsState.dragPayload.node.meta.dir_name,
//           }
//         : {
//               type: "request",
//               file_name: treeActionsState.dragPayload.node.meta.file_name,
//           };
//     const removeTreeNodeFromCollectionResult = removeTreeNodeFromCollection({
//         parentRelativePath: treeActionsState.dragPayload.parentRelativePath,
//         removeTreeNodeDto,
//         mutRootCollection,
//     });
//     if (removeTreeNodeFromCollectionResult.status === "error") {
//         console.error("Unable to remove tree item from the collection");
//         return;
//     }

//     const fileOrDirName = isCol(treeActionsState.dragPayload.node)
//         ? treeActionsState.dragPayload.node.meta.dir_name
//         : treeActionsState.dragPayload.node.meta.file_name;
//     const moveTreeNodeDto: MoveTreeNodeDto = {
//         src_relpath: buildPath(treeActionsState.dragPayload.parentRelativePath, fileOrDirName),
//         dest_relpath: buildPath(treeActionsState.dropTargetPath, fileOrDirName),
//     };
//     const moveTreeNodeResult = await commands.moveTreeitem(moveTreeNodeDto);
//     if (moveTreeNodeResult.status === "error") {
//         console.error(moveTreeNodeResult.error);
//         toast.error(
//             `Something went wrong. Unable to move \`${treeActionsState.dragPayload.node.meta.name}\``,
//         );

//         return;
//     }

//     treeActionsState.dragPayload = null;
//     treeActionsState.dropTargetPath = null;
// }

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

export function buildPath(currentPath: string, treeNodeName: string) {
    return currentPath === RELATIVE_SPACE_ROOT ? treeNodeName : `${currentPath}/${treeNodeName}`;
}

export function isCurrentCollectionOrAnyOfItsChildFocussed(currentPath: string): boolean {
    const isCurrentCollectionFocussed =
        treeNodesState.focussedNode.type === "collection" &&
        treeNodesState.focussedNode.relativePath === currentPath;
    const isCurrentCollectionChildFocussed =
        treeNodesState.focussedNode.type === "request" &&
        treeNodesState.focussedNode.parentRelativePath === currentPath;

    return isCurrentCollectionFocussed || isCurrentCollectionChildFocussed;
}

// export type AddTreeNodeToCollectionParams = {
//     parentRelativePath: string;
//     treeNode: TreeNode;
//     targetPath: string;
//     mutRootCollection: Collection;
// };

// export function addTreeNodeToCollection({
//     parentRelativePath,
//     treeNode,
//     targetPath,
//     mutRootCollection,
// }: AddTreeNodeToCollectionParams): Result<void, void> {
//     if (targetPath === parentRelativePath) {
//         console.warn(`Abort dropping to the same parent \`${parentRelativePath}\``);
//         return err();
//     }

//     let current: Collection = mutRootCollection;
//     let traversedPath = "/";

//     for (const segment of pathSegments(targetPath)) {
//         const nextCollection = current.collections.find(
//             collection => collection.meta.dir_name === segment,
//         );
//         if (!nextCollection) {
//             console.warn(`Target collection \`${segment}\` not found in \`${traversedPath}\``);
//             return err();
//         }

//         current = nextCollection;
//         traversedPath =
//             traversedPath === "/"
//                 ? traversedPath.concat(segment)
//                 : traversedPath.concat("/").concat(segment);
//     }

//     if (isCol(treeNode)) {
//         const relativePath =
//             parentRelativePath === RELATIVE_SPACE_ROOT
//                 ? parentRelativePath.concat(treeNode.meta.dir_name)
//                 : parentRelativePath.concat("/").concat(treeNode.meta.dir_name);

//         if (isSubPath(relativePath, targetPath)) {
//             console.warn(
//                 `Abort moving collection to itself or it's own child collection \`${targetPath}\``,
//             );
//             return err();
//         }
//     } else {
//         if (parentRelativePath === traversedPath) {
//             console.warn(`Abort moving request into the same collection \`${targetPath}\``);
//             return err();
//         }
//     }

//     if (isCol(treeNode)) {
//         const collectionDirNameAlreadyExists = current.collections.some(
//             collection => collection.meta.dir_name === treeNode.meta.dir_name,
//         );
//         if (collectionDirNameAlreadyExists) {
//             toast.error(
//                 `Collection with directory name ${treeNode.meta.dir_name} already exists in the ${current.meta.dir_name} collection`,
//             );

//             return err();
//         }
//         current.collections.push(treeNode);
//         current.collections.sort((a, b) =>
//             a.meta.dir_name.toLocaleLowerCase().localeCompare(b.meta.dir_name),
//         );

//         return ok();
//     } else {
//         const reqFileNameAlreadyExists = current.requests.some(
//             request => request.meta.file_name === treeNode.meta.file_name,
//         );
//         if (reqFileNameAlreadyExists) {
//             toast.error(
//                 `Request with file name ${treeNode.meta.file_name} already exists in the ${current.meta.dir_name} collection`,
//             );

//             return err();
//         }
//         current.requests.push(treeNode);
//         current.requests.sort((a, b) => a.meta.file_name.localeCompare(b.meta.file_name));

//         return ok();
//     }
// }

// export type RemoveTreeNodeFromCollectionParams = {
//     parentRelativePath: string;
//     removeTreeNodeDto: RemoveTreeNodeDto;
//     mutRootCollection: Collection;
// };

// export function removeTreeNodeFromCollection({
//     parentRelativePath,
//     removeTreeNodeDto,
//     mutRootCollection,
// }: RemoveTreeNodeFromCollectionParams): Result<void, void> {
//     const segments = pathSegments(parentRelativePath);
//     let current: Collection = mutRootCollection;

//     for (const segment of segments) {
//         const nextCollection = current.collections.find(
//             collection => collection.meta.dir_name === segment,
//         );
//         if (!nextCollection) {
//             console.warn(`Collection not found for segment: ${segment}`);

//             return err();
//         }

//         current = nextCollection;
//     }

//     if (removeTreeNodeDto.type === "collection") {
//         current.collections = current.collections.filter(
//             collection => collection.meta.dir_name !== removeTreeNodeDto.dir_name,
//         );

//         return ok();
//     } else {
//         current.requests = current.requests.filter(
//             request => request.meta.file_name !== removeTreeNodeDto.file_name,
//         );

//         return ok();
//     }
// }
