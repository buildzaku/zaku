import { Tooltip as TooltipPrimitive } from "bits-ui";
import Content from "./tooltip-content.svelte";

const Root: typeof TooltipPrimitive.Root = TooltipPrimitive.Root;
const Trigger: typeof TooltipPrimitive.Trigger = TooltipPrimitive.Trigger;
const Provider: typeof TooltipPrimitive.Provider = TooltipPrimitive.Provider;

export {
  Root,
  Trigger,
  Content,
  Provider,
  //
  Root as Tooltip,
  Content as TooltipContent,
  Trigger as TooltipTrigger,
  Provider as TooltipProvider,
};
