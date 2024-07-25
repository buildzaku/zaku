import { writable } from "svelte/store";

export const workspaces = writable<[]>([]);

export const collections = writable<[]>([]);

export type Collection = {
    id: string;
    name: string;
    description: string;
    requests: [];
};

export type Request = {
    id: string;
    name: string;
    description: string;
    config: RequestConfig | null;
};

export type RequestConfig = HttpRequestConfig;

export type HttpRequestConfig = {
    type: "http";
    method: string;
};
