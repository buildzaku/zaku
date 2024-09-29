export type TreeItem = Collection | Request;

export type Collection = {
    meta: CollectionMeta;
    requests: Request[];
    collections: Collection[];
};

export type CollectionMeta = {
    folder_name: string;
    display_name: string | null;
    is_open: boolean;
};

export type Request = {
    meta: RequestMeta;
    config: RequestConfig;
};

export type RequestConfig = {
    method: string;
    url?: string;
};

export type RequestMeta = {
    file_name: string;
    display_name: string;
};

export type Space = {
    absolute_path: string;
    meta: SpaceMeta;
    root: Collection;
};

export type SpaceMeta = {
    name: string;
};

export type SpaceReference = {
    name: string;
    path: string;
};

export type CreateSpaceDto = {
    name: string;
    location: string;
};

export type ZakuState = {
    active_space: Space | null;
    space_references: SpaceReference[];
};

export type ZakuError = {
    error: string;
    message: string;
};

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
