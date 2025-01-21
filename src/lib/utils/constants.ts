export const METHODS = {
    Get: "GET",
    Post: "POST",
    Put: "PUT",
    Patch: "PATCH",
    Delete: "DELETE",
    Head: "HEAD",
    Options: "OPTIONS",
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

export const RELATIVE_SPACE_ROOT = "";
