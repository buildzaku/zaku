import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
import { tv } from "tailwind-variants";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export const requestColors = tv({
  base: "",
  variants: {
    method: {
      GET: "text-method-get data-[highlighted]:text-method-get",
      POST: "text-method-post data-[highlighted]:text-method-post",
      DELETE: "text-method-delete data-[highlighted]:text-method-delete",
      PUT: "text-method-put data-[highlighted]:text-method-put",
      PATCH: "text-method-patch data-[highlighted]:text-method-patch",
      HEAD: "text-method-head data-[highlighted]:text-method-head",
      OPTIONS: "text-method-options data-[highlighted]:text-method-options",
      default: "text-method-default data-[highlighted]:text-method-default",
    },
  },
  defaultVariants: {
    method: "default",
  },
}) as (props: { method: string }) => string;

export type WithoutChild<T> = T extends { child?: any } ? Omit<T, "child"> : T;
export type WithoutChildren<T> = T extends { children?: any } ? Omit<T, "children"> : T;
export type WithoutChildrenOrChild<T> = WithoutChildren<WithoutChild<T>>;
export type WithElementRef<T, U extends HTMLElement = HTMLElement> = T & { ref?: U | null };

export function capitalizeFirst(str: string) {
  return str.charAt(0).toUpperCase() + str.slice(1);
}
