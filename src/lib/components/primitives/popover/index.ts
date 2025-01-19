import { Popover as PopoverPrimitive } from "bits-ui";
import Content from "./popover-content.svelte";

const Root: typeof PopoverPrimitive.Root = PopoverPrimitive.Root;
const Trigger: typeof PopoverPrimitive.Trigger = PopoverPrimitive.Trigger;
const Close: typeof PopoverPrimitive.Close = PopoverPrimitive.Close;

export {
    Root,
    Content,
    Trigger,
    Close,
    //
    Root as Popover,
    Content as PopoverContent,
    Trigger as PopoverTrigger,
    Close as PopoverClose,
};
