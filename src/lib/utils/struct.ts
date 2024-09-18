import { array, minLength, parse, pipe, safeParse, strictObject, string, nullable } from "valibot";

export const Struct = {
    parse,
    safeParse,
    pipe,
    string,
    minLength,
    strictObject,
    array,
    nullable,
};

export type ValueOf<T> = T[keyof T];

export type Ok<T> = {
    ok: true;
    value: T;
};

export function Ok<T = void>(value?: T): Ok<T> {
    return { ok: true, value: value as T };
}

export type Err<E> = {
    ok: false;
    err: E;
};

export function Err<E>(err: E): Err<E> {
    return { ok: false, err };
}

export type Result<T, E = unknown> = Ok<T> | Err<E>;

export const safeRun =
    <TArgs extends any[], TReturn, E = unknown>(func: (..._args: TArgs) => TReturn) =>
    (...args: TArgs): Result<TReturn, E> => {
        try {
            return Ok(func(...args));
        } catch (err) {
            return Err(err as E);
        }
    };

export const safeRunAsync =
    <TArgs extends any[], TReturn, E = unknown>(func: (..._args: TArgs) => Promise<TReturn>) =>
    async (...args: TArgs): Promise<Result<TReturn, E>> => {
        try {
            const result = await func(...args);
            return Ok(result);
        } catch (err) {
            return Err(err as E);
        }
    };
