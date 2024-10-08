import type { Collection, Request } from "$lib/bindings";
import type { ValueOf } from "$lib/utils";

export type TreeItem = Collection | Request;

export type DragPayload = {
    parentRelativePath: string;
    treeItem: TreeItem;
};

export type DragOverDto =
    | {
          type: "collection";
          relativePath: string;
      }
    | {
          type: "request";
          parentRelativePath: string;
      };

export type RemoveTreeItemDto =
    | {
          type: "collection";
          dir_name: string;
      }
    | {
          type: "request";
          file_name: string;
      };

export const TREE_ITEM_TYPE = {
    Collection: "collection",
    Request: "request",
} as const;

export type FocussedTreeItem = {
    type: ValueOf<typeof TREE_ITEM_TYPE>;
    parentRelativePath: string;
    relativePath: string;
};
