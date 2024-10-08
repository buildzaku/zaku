// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.

export type Collection = {
    meta: CollectionMeta;
    requests: Array<Request>;
    collections: Array<Collection>;
};

export type CollectionMeta = { dir_name: string; display_name: string | null; is_open: boolean };

export type CreateCollectionDto = { parent_relative_path: string; relative_path: string };

export type CreateNewCollectionOrRequest = { parent_relative_path: string; relative_path: string };

export type CreateRequestDto = { parent_relative_path: string; relative_path: string };

export type CreateSpaceDto = { name: string; location: string };

export type DispatchNotificationOptions = { title: string; body: string };

export type OpenDirectoryDialogOptions = { title: string | null };

export type Request = { meta: RequestMeta; config: RequestConfig };

export type RequestConfig = { method: string; url: string | null };

export type RequestMeta = { file_name: string; display_name: string };

export type Space = { absolute_path: string; meta: SpaceMeta; root: Collection };

export type SpaceMeta = { name: string };

export type SpaceReference = { path: string; name: string };

export type ZakuError = { error: string; message: string };

export type ZakuState = { active_space: Space | null; space_references: Array<SpaceReference> };
