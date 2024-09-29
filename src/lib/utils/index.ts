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

export function Err<E = void>(err?: E): Err<E> {
    return { ok: false, err: err as E };
}

export type Result<T, E = void> = Ok<T> | Err<E>;

export function isObject(value: unknown): value is object {
    return typeof value === "object" && value !== null;
}

export function safeRun<TReturn, E = unknown>(result: TReturn): Result<TReturn, E> {
    try {
        return Ok(result);
    } catch (err) {
        return Err(err as E);
    }
}

export async function safeRunAsync<TReturn, E = unknown>(
    promise: Promise<TReturn>,
): Promise<Result<TReturn, E>> {
    try {
        const result = await promise;
        return Ok(result);
    } catch (err) {
        return Err(err as E);
    }
}
