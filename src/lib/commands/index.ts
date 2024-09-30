import { invoke } from "@tauri-apps/api/core";
import type { InvokeArgs, InvokeOptions } from "@tauri-apps/api/core";

import { Err, isObject, Ok } from "$lib/utils";
import type { Result } from "$lib/utils";
import type { SpaceReference, ZakuError } from "$lib/bindings";

export type OpenDirectoryDialogOptions = {
    title?: string;
};

export async function openDirectoryDialog(options?: OpenDirectoryDialogOptions) {
    const path: string | null = await invoke("open_directory_dialog", { options });

    return path;
}

export type DispatchNotificationOptions = {
    title: string;
    body: string;
};

export async function dispatchNotification(options: DispatchNotificationOptions) {
    const isNotificationPermissionGranted: boolean = await invoke(
        "is_notification_permission_granted",
    );

    if (isNotificationPermissionGranted) {
        await invoke("dispatch_notification", { options });
    } else {
        const isNotificationPermissionGrantedAfterRequest: boolean = await invoke(
            "request_notification_permission",
        );

        if (isNotificationPermissionGrantedAfterRequest) {
            await invoke("dispatch_notification", { options });
        }
    }
}

export async function getSpaceReference(path: string): Promise<SpaceReference> {
    const spaceReference = await invoke<SpaceReference>("get_space_reference", { path });

    return spaceReference;
}

export type InvokeCommand = "get_zaku_state" | "set_active_space";

export async function safeInvoke<T>(
    command: InvokeCommand,
    args?: InvokeArgs,
    options?: InvokeOptions,
): Promise<Result<T, ZakuError>> {
    try {
        const result = await invoke<T>(command, args, options);

        return Ok(result);
    } catch (err) {
        const zakuError: ZakuError = {
            error: isObject(err) && "error" in err ? String(err.error) : "Unknown",
            message:
                isObject(err) && "message" in err ? String(err.message) : "Something went wrong.",
        };

        return Err(zakuError);
    }
}
