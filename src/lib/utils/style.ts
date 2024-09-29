import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
    return twMerge(clsx(inputs));
}

export const getMethodColorClass = (method: string) => {
    switch (method) {
        case "GET":
            return "text-emerald-300";
        case "POST":
            return "text-orange-300";
        case "DELETE":
            return "text-red-300";
        case "PUT":
            return "text-blue-300";
        case "PATCH":
            return "text-indigo-300";
        case "HEAD":
            return "text-amber-300";
        case "OPTIONS":
            return "text-fuchsia-300";
        default:
            return "text-gray-300";
    }
};
