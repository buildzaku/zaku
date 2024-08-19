import { invoke } from "@tauri-apps/api/core";

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
