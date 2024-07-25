export const METHODS = {
    GET: { value: "GET", label: "GET" },
    POST: { value: "POST", label: "POST" },
    PUT: { value: "PUT", label: "PUT" },
    PATCH: { value: "PATCH", label: "PATCH" },
    DELETE: { value: "DELETE", label: "DELETE" },
    HEAD: { value: "HEAD", label: "HEAD" },
    OPTIONS: { value: "OPTIONS", label: "OPTIONS" },
} as const;

export const METHOD_CLASS = {
    GET: "text-method-get data-[highlighted]:text-method-get",
    POST: "text-method-post data-[highlighted]:text-method-post",
    PUT: "text-method-put data-[highlighted]:text-method-put",
    PATCH: "text-method-patch data-[highlighted]:text-method-patch",
    DELETE: "text-method-delete data-[highlighted]:text-method-delete",
    HEAD: "text-method-head data-[highlighted]:text-method-head",
    OPTIONS: "text-method-options data-[highlighted]:text-method-options",
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
