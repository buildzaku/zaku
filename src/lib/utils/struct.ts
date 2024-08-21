import { array, minLength, parse, pipe, safeParse, strictObject, string } from "valibot";

export const Struct = {
    parse,
    safeParse,
    pipe,
    string,
    minLength,
    strictObject,
    array,
};

export type ValueOf<T> = T[keyof T];

export type { InferInput } from "valibot";
