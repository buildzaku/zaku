import type { Collection, HttpReq } from "$lib/bindings";

export type TreeNode = Collection | HttpReq;

export type DragPayload = {
    parentRelativePath: string;
    node: TreeNode;
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

export type FocussedTreeNode =
    | {
          type: "collection";
          parentRelativePath: string;
          relativePath: string;
      }
    | {
          type: "request";
          parentRelativePath: string;
          relativePath: string;
      };

export type OpenRequest = {
    parentRelpath: string;
    parentNames: string[];
    self: HttpReq;
};
