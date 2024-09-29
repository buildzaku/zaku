export const METHODS = {
    GET: { value: "GET", label: "GET" },
    POST: { value: "POST", label: "POST" },
    PUT: { value: "PUT", label: "PUT" },
    PATCH: { value: "PATCH", label: "PATCH" },
    DELETE: { value: "DELETE", label: "DELETE" },
    HEAD: { value: "HEAD", label: "HEAD" },
    OPTIONS: { value: "OPTIONS", label: "OPTIONS" },
} as const;

export const REQUEST_BODY_TYPES = {
    None: "None",
    Json: "application/json",
    Xml: "application/xml",
    FormUrlEncoded: "application/x-www-form-urlencoded",
    Binary: "application/octet-stream",
    FormData: "multipart/form-data",
    Html: "text/html",
    PlainText: "text/plain",
} as const;

export const RELATIVE_SPACE_ROOT = "/";
