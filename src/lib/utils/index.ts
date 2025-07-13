import type { Result } from "$lib/bindings";

export type ValueOf<T> = T[keyof T];

export function ok<T = void>(value?: T): Result<T, never> {
    return { status: "ok", data: value as T };
}

export function err<E = void>(err?: E): Result<never, E> {
    return { status: "error", error: err as E };
}

export function prettyJson(data: string | undefined) {
    if (!data) return String();

    try {
        return JSON.stringify(JSON.parse(data), null, 2);
    } catch {
        return data;
    }
}

export function formatElapsed(ms: number): string {
    if (ms < 1000) return `${ms} ms`;

    const seconds = ms / 1000;
    if (seconds < 60) {
        return seconds % 1 === 0 ? `${seconds}s` : `${seconds.toFixed(2).replace(/\.?0+$/, "")} s`;
    }

    const minutes = Math.floor(seconds / 60);
    const secRemainder = Math.floor(seconds % 60);
    if (minutes < 60) {
        return `${minutes} m ${secRemainder} s`;
    }

    const hours = Math.floor(minutes / 60);
    const minRemainder = minutes % 60;

    return `${hours} h ${minRemainder} m`;
}

export function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;

    const kb = bytes / 1024;
    if (kb < 1024) {
        return kb % 1 === 0 ? `${kb} KB` : `${kb.toFixed(2).replace(/\.?0+$/, "")} KB`;
    }

    const mb = kb / 1024;
    if (mb < 1024) {
        return mb % 1 === 0 ? `${mb} MB` : `${mb.toFixed(2).replace(/\.?0+$/, "")} MB`;
    }

    const gb = mb / 1024;

    return gb % 1 === 0 ? `${gb} GB` : `${gb.toFixed(2).replace(/\.?0+$/, "")} GB`;
}
