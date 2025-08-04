import { DropdownMenu as DropdownMenuPrimitive } from "bits-ui";
import CheckboxItem from "./dropdown-menu-checkbox-item.svelte";
import Content from "./dropdown-menu-content.svelte";
import GroupHeading from "./dropdown-menu-group-heading.svelte";
import Item from "./dropdown-menu-item.svelte";
import Label from "./dropdown-menu-label.svelte";
import RadioItem from "./dropdown-menu-radio-item.svelte";
import Separator from "./dropdown-menu-separator.svelte";
import Shortcut from "./dropdown-menu-shortcut.svelte";
import SubContent from "./dropdown-menu-sub-content.svelte";
import SubTrigger from "./dropdown-menu-sub-trigger.svelte";

const Sub: typeof DropdownMenuPrimitive.Sub = DropdownMenuPrimitive.Sub;
const Root: typeof DropdownMenuPrimitive.Root = DropdownMenuPrimitive.Root;
const Trigger: typeof DropdownMenuPrimitive.Trigger = DropdownMenuPrimitive.Trigger;
const Group: typeof DropdownMenuPrimitive.Group = DropdownMenuPrimitive.Group;
const RadioGroup: typeof DropdownMenuPrimitive.RadioGroup = DropdownMenuPrimitive.RadioGroup;

export {
  CheckboxItem,
  Content,
  Root as DropdownMenu,
  CheckboxItem as DropdownMenuCheckboxItem,
  Content as DropdownMenuContent,
  Group as DropdownMenuGroup,
  GroupHeading as DropdownMenuGroupHeading,
  Item as DropdownMenuItem,
  Label as DropdownMenuLabel,
  RadioGroup as DropdownMenuRadioGroup,
  RadioItem as DropdownMenuRadioItem,
  Separator as DropdownMenuSeparator,
  Shortcut as DropdownMenuShortcut,
  Sub as DropdownMenuSub,
  SubContent as DropdownMenuSubContent,
  SubTrigger as DropdownMenuSubTrigger,
  Trigger as DropdownMenuTrigger,
  Group,
  GroupHeading,
  Item,
  Label,
  RadioGroup,
  RadioItem,
  Root,
  Separator,
  Shortcut,
  Sub,
  SubContent,
  SubTrigger,
  Trigger,
};
