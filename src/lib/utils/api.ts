import { version } from "$app/environment";

export type RequestStatus = "idle" | "loading" | "success" | "error";

export const BASE_REQUEST_HEADERS: [boolean, string, string][] = [
    [true, "Cache-Control", "no-cache"],
    [true, "User-Agent", `Zaku/${version}`],
];
