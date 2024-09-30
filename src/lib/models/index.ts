import type { Collection, Request } from "$lib/bindings";

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
          folder_name: string;
      }
    | {
          type: "request";
          file_name: string;
      };
