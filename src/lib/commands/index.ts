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
    await invoke("dispatch_notification", { options });
}
