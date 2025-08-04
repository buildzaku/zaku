import type { Collection, HttpReq } from "$lib/bindings";
import { Path } from "$lib/utils/path";

export type TreeNode = Collection | HttpReq;

export type DragOverDto = {
  type: "collection" | "request";
  relpath: Path;
};

export type FocussedTreeNode = {
  type: "collection" | "request";
  relpath: Path;
};

export type OpenRequest = {
  trail: string[];
  self: HttpReq;
};
