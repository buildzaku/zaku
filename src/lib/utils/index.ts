import type { Result } from "$lib/bindings";

export type ValueOf<T> = T[keyof T];

export function Ok<T = void>(value?: T): Result<T, never> {
    return { status: "ok", data: value as T };
}

export function Err<E = void>(err?: E): Result<never, E> {
    return { status: "error", error: err as E };
}
