import { invoke } from "@tauri-apps/api/core";
import { isValiError, parse as vParse } from "valibot";
import type { InvokeArgs, InvokeOptions } from "@tauri-apps/api/core";
import type { BaseIssue, BaseSchema, InferOutput } from "valibot";

import { spaceReferenceStruct, zakuErrorStruct } from "$lib/store";
import { Err, Ok, Struct } from "$lib/utils/struct";
import type { SpaceReference, ZakuError } from "$lib/store";
import type { Result } from "$lib/utils/struct";

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
    const spaceReferenceRawResult: boolean = await invoke("get_space_reference", { path });

    return Struct.parse(spaceReferenceStruct, spaceReferenceRawResult);
}

export function safeParse<const TSchema extends BaseSchema<unknown, unknown, BaseIssue<unknown>>>(
    schema: TSchema,
    input: unknown,
): Result<InferOutput<TSchema>> {
    try {
        const parsedResult = vParse(schema, input);

        return Ok(parsedResult);
    } catch (err) {
        return Err(err);
    }
}

export type InvokeCommand = "get_zaku_state" | "set_active_space";

export async function safeInvoke<
    const TSchema extends BaseSchema<unknown, unknown, BaseIssue<unknown>>,
>(
    schema: TSchema,
    command: InvokeCommand,
    args?: InvokeArgs,
    options?: InvokeOptions,
): Promise<Result<InferOutput<TSchema>, ZakuError>> {
    try {
        const result = await invoke(command, args, options);
        const parsedResult = vParse(schema, result);

        return Ok(parsedResult);
    } catch (err) {
        if (isValiError(err)) {
            return Err({ error: err.message, message: "Something went wrong." });
        } else {
            const zakuError = safeParse(zakuErrorStruct, err);
            const parsedZakuError: ZakuError = zakuError.ok
                ? zakuError.value
                : { error: "Unknown", message: "Something went wrong." };

            return Err(parsedZakuError);
        }
    }
}
