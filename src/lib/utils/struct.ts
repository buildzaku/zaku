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

export type { InferInput } from "valibot";
