import { toast } from "svelte-sonner";

import type { TreeItem, RemoveTreeItemDto } from "$lib/models";
import { RELATIVE_SPACE_ROOT } from "$lib/utils/constants";
import { Ok, Err } from "$lib/utils";
import type { Result } from "$lib/utils";
import type { Collection, Request } from "$lib/bindings";

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

export type AddTreeItemToCollectionParams = {
    parentRelativePath: string;
    treeItem: Collection | Request;
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
        const requestFileNameAlreadyExists = current.requests.some(
            request => request.meta.file_name === treeItem.meta.file_name,
        );
        if (requestFileNameAlreadyExists) {
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
