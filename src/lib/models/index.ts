import type { Collection, Request } from "$lib/bindings";

export type ZkResponse = {
    status: number;
    data: string;
};

export type TreeItem = Collection | Request;

export type DragPayload = {
    parentRelativePath: string;
    treeItem: TreeItem;
};

export enum TreeItemType {
    Collection = "collection",
    Request = "request",
}

export type DragOverDto =
    | {
          type: TreeItemType.Collection;
          relativePath: string;
      }
    | {
          type: TreeItemType.Request;
          parentRelativePath: string;
      };

export type RemoveTreeItemDto =
    | {
          type: TreeItemType.Collection;
          dir_name: string;
      }
    | {
          type: TreeItemType.Request;
          file_name: string;
      };

export type FocussedTreeItem =
    | {
          type: TreeItemType.Collection;
          parentRelativePath: string;
          relativePath: string;
      }
    | {
          type: TreeItemType.Request;
          parentRelativePath: string;
          relativePath: string;
      };

export type ActiveRequest = {
    parentRelativePath: string;
    self: Request;
};
