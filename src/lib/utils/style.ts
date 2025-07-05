import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
    return twMerge(clsx(inputs));
}

export const getMethodColorClass = (method: string) => {
    switch (method) {
        case "GET":
            return "text-emerald-300 data-[highlighted]:text-emerald-300";
        case "POST":
            return "text-orange-300 data-[highlighted]:text-orange-300";
        case "DELETE":
            return "text-red-300 data-[highlighted]:text-red-300";
        case "PUT":
            return "text-blue-300 data-[highlighted]:text-blue-300";
        case "PATCH":
            return "text-indigo-300 data-[highlighted]:text-indigo-300";
        case "HEAD":
            return "text-amber-300 data-[highlighted]:text-amber-300";
        case "OPTIONS":
            return "text-fuchsia-300 data-[highlighted]:text-fuchsia-300";
        default:
            return "text-gray-300 data-[highlighted]:text-gray-300";
    }
};

export type WithoutChild<T> = T extends { child?: any } ? Omit<T, "child"> : T;
export type WithoutChildren<T> = T extends { children?: any } ? Omit<T, "children"> : T;
export type WithoutChildrenOrChild<T> = WithoutChildren<WithoutChild<T>>;
export type WithElementRef<T, U extends HTMLElement = HTMLElement> = T & { ref?: U | null };
