import { invoke } from "@tauri-apps/api/core";

export type OpenDirectoryDialog = {
    title?: string;
};

export async function openDirectoryDialog(options?: OpenDirectoryDialog) {
    const path: string | null = await invoke("open_directory_dialog", { options });

    return path;
}
