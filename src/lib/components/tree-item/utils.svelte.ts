import { mount, unmount } from "svelte";
import { toast } from "svelte-sonner";

import { TreeNodePreview } from "$lib/components/tree-item";
import { sharedState, explorerActionsState, explorerState } from "$lib/state.svelte";
import type { DragOverDto, DragPayload, TreeNode } from "$lib/models";
import { RELATIVE_SPACE_ROOT } from "$lib/utils/constants";
import { commands } from "$lib/bindings";
import type { Collection, HttpReq } from "$lib/bindings";

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

export function joinPaths(paths: string[]) {
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

    const src_relpath = buildPath(
        explorerActionsState.dragPayload.parentRelativePath,
        explorerActionsState.dragPayload.node.meta.fsname,
    );
    const dest_relpath = buildPath(
        explorerActionsState.dropTargetPath,
        explorerActionsState.dragPayload.node.meta.fsname,
    );

    const treeNodeDropResult = await commands.handleTreeNodeDrop({
        node_type: isCol(explorerActionsState.dragPayload.node) ? "collection" : "request",
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

//     if (sharedState.space === null) {
//         console.warn("Active space not found");
//         return;
//     }
//     if (explorerActionsState.dragPayload === null) {
//         console.warn("Drag payload not found");
//         return;
//     }
//     if (explorerActionsState.dropTargetPath === null) {
//         console.warn("Drop target path not found");
//         return;
//     }

//     const mutRootCollection = sharedState.space.root_collection;
//     const addTreeNodeToCollectionResult = addTreeNodeToCollection({
//         parentRelativePath: explorerActionsState.dragPayload.parentRelativePath,
//         treeNode: explorerActionsState.dragPayload.node,
//         targetPath: explorerActionsState.dropTargetPath,
//         mutRootCollection,
//     });
//     if (addTreeNodeToCollectionResult.status === "error") {
//         console.error("Cannot add tree item to the collection");
//         return;
//     }

//     const removeTreeNodeDto: RemoveTreeNodeDto = isCol(explorerActionsState.dragPayload.node)
//         ? {
//               type: "collection",
//               fsname: explorerActionsState.dragPayload.node.meta.fsname,
//           }
//         : {
//               type: "request",
//               fsname: explorerActionsState.dragPayload.node.meta.fsname,
//           };
//     const removeTreeNodeFromCollectionResult = removeTreeNodeFromCollection({
//         parentRelativePath: explorerActionsState.dragPayload.parentRelativePath,
//         removeTreeNodeDto,
//         mutRootCollection,
//     });
//     if (removeTreeNodeFromCollectionResult.status === "error") {
//         console.error("Unable to remove tree item from the collection");
//         return;
//     }

//     const fileOrDirName = isCol(explorerActionsState.dragPayload.node)
//         ? explorerActionsState.dragPayload.node.meta.fsname
//         : explorerActionsState.dragPayload.node.meta.fsname;
//     const moveTreeNodeDto: MoveTreeNodeDto = {
//         src_relpath: buildPath(explorerActionsState.dragPayload.parentRelativePath, fileOrDirName),
//         dest_relpath: buildPath(explorerActionsState.dropTargetPath, fileOrDirName),
//     };
//     const moveTreeNodeResult = await commands.moveTreeitem(moveTreeNodeDto);
//     if (moveTreeNodeResult.status === "error") {
//         console.error(moveTreeNodeResult.error);
//         toast.error(
//             `Something went wrong. Unable to move \`${explorerActionsState.dragPayload.node.meta.name}\``,
//         );

//         return;
//     }

//     explorerActionsState.dragPayload = null;
//     explorerActionsState.dropTargetPath = null;
// }

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
//             collection => collection.meta.fsname === segment,
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
//                 ? parentRelativePath.concat(treeNode.meta.fsname)
//                 : parentRelativePath.concat("/").concat(treeNode.meta.fsname);

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
//             collection => collection.meta.fsname === treeNode.meta.fsname,
//         );
//         if (collectionDirNameAlreadyExists) {
//             toast.error(
//                 `Collection with directory name ${treeNode.meta.fsname} already exists in the ${current.meta.fsname} collection`,
//             );

//             return err();
//         }
//         current.collections.push(treeNode);
//         current.collections.sort((a, b) =>
//             a.meta.fsname.toLocaleLowerCase().localeCompare(b.meta.fsname),
//         );

//         return ok();
//     } else {
//         const reqFileNameAlreadyExists = current.requests.some(
//             request => request.meta.fsname === treeNode.meta.fsname,
//         );
//         if (reqFileNameAlreadyExists) {
//             toast.error(
//                 `Request with file name ${treeNode.meta.fsname} already exists in the ${current.meta.fsname} collection`,
//             );

//             return err();
//         }
//         current.requests.push(treeNode);
//         current.requests.sort((a, b) => a.meta.fsname.localeCompare(b.meta.fsname));

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
//             collection => collection.meta.fsname === segment,
//         );
//         if (!nextCollection) {
//             console.warn(`Collection not found for segment: ${segment}`);

//             return err();
//         }

//         current = nextCollection;
//     }

//     if (removeTreeNodeDto.type === "collection") {
//         current.collections = current.collections.filter(
//             collection => collection.meta.fsname !== removeTreeNodeDto.fsname,
//         );

//         return ok();
//     } else {
//         current.requests = current.requests.filter(
//             request => request.meta.fsname !== removeTreeNodeDto.fsname,
//         );

//         return ok();
//     }
// }
