<script lang="ts">
  import { mode, setMode } from "mode-watcher";
  import { toast } from "svelte-sonner";

  import { Button } from "$lib/components/primitives/button";
  import {
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogContent,
    DialogFooter,
  } from "$lib/components/primitives/dialog";
  import { Tabs, TabsList, TabsTrigger, TabsContent } from "$lib/components/primitives/tabs";
  import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
  } from "$lib/components/primitives/select";
  import { commands } from "$lib/bindings";
  import { Checkbox } from "$lib/components/primitives/checkbox";
  import { Label } from "$lib/components/primitives/label";
  import { emitCmdError } from "$lib/utils";
  import { appStateRx } from "$lib/state.svelte";
  import { capitalizeFirst } from "$lib/utils/style";

  const tabs = [
    { value: "preferences", label: "Preferences" },
    { value: "space", label: "Space" },
  ] as const;

  let spaceSettingsStr: string = $state(
    appStateRx.space ? JSON.stringify(appStateRx.space.settings) : String(),
  );
  let userSettingsStr: string = $state(
    appStateRx.userSettings ? JSON.stringify(appStateRx.userSettings) : String(),
  );

  async function handeSave() {
    if (!appStateRx.space) return;

    if (spaceSettingsStr !== JSON.stringify(appStateRx.space.settings)) {
      const saveSpaceResult = await commands.saveSpaceSettings(
        appStateRx.space.abspath,
        appStateRx.space.settings,
      );
      if (saveSpaceResult.status !== "ok") {
        return emitCmdError(saveSpaceResult.error);
      }
      spaceSettingsStr = JSON.stringify(appStateRx.space.settings);
    }

    if (appStateRx.userSettings && userSettingsStr !== JSON.stringify(appStateRx.userSettings)) {
      const saveUserResult = await commands.saveUserSettings(appStateRx.userSettings);
      if (saveUserResult.status !== "ok") {
        return emitCmdError(saveUserResult.error);
      }
      userSettingsStr = JSON.stringify(appStateRx.userSettings);
    }

    if (mode.current !== appStateRx.space.settings.theme) {
      setMode(appStateRx.space.settings.theme);
    }

    toast.success(`Settings saved successfully`);
  }
</script>

{#if appStateRx.space}
  <DialogContent class="flex h-[80%] max-h-[80%] w-[80%] max-w-[80%] flex-col">
    <DialogHeader>
      <DialogTitle>Settings</DialogTitle>
      <DialogDescription>Manage application and space settings</DialogDescription>
    </DialogHeader>
    <Tabs
      value="preferences"
      orientation="vertical"
      class="flex h-full max-h-[calc(100%-1.5rem)] overflow-hidden"
    >
      <div class="w-48 flex-shrink-0 p-0.5">
        <TabsList class="flex h-auto w-full flex-col gap-1 bg-transparent p-0">
          {#each tabs as tab (tab.value)}
            <TabsTrigger value={tab.value} class="w-full justify-start px-3 py-2">
              {tab.label}
            </TabsTrigger>
          {/each}
        </TabsList>
      </div>

      <div class="mx-3 h-full w-px"></div>

      <div class="flex-1 overflow-y-auto">
        <TabsContent value="preferences" class="m-0 h-full">
          <div class="space-y-6">
            <div>
              <h3 class="text-lg font-medium">Preferences</h3>
              <p class="text-muted-foreground mb-4 text-sm">Configure global preferences</p>
            </div>

            {#if appStateRx.userSettings}
              <div>
                <h4 class="text-medium mb-3 leading-none font-semibold tracking-tight">
                  Appearance
                </h4>
                <div class="flex items-center gap-3">
                  <Label for="default-theme" class="text-sm font-medium">Default Theme</Label>
                  <Select type="single" bind:value={appStateRx.userSettings.default_theme}>
                    <SelectTrigger id="default-theme" class="w-32">
                      <span>{capitalizeFirst(appStateRx.userSettings.default_theme)}</span>
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="system">System</SelectItem>
                      <SelectItem value="light">Light</SelectItem>
                      <SelectItem value="dark">Dark</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <p class="text-muted-foreground mt-2 text-xs">
                  Choose the default theme for new spaces. System will follow your OS preference
                </p>
              </div>
            {/if}
          </div>
        </TabsContent>
        <TabsContent value="space" class="m-0 h-full">
          <div class="space-y-6">
            <div>
              <h3 class="text-lg font-medium">Space Settings</h3>
              <p class="text-muted-foreground mb-4 text-sm">
                Configure settings specific to this space
              </p>
            </div>

            <div>
              <h4 class="text-medium mb-3 leading-none font-semibold tracking-tight">Appearance</h4>
              <div class="flex items-center gap-3">
                <Label for="space-theme" class="text-sm font-medium">Theme</Label>
                <Select type="single" bind:value={appStateRx.space.settings.theme}>
                  <SelectTrigger id="space-theme" class="w-32">
                    <span>{capitalizeFirst(appStateRx.space.settings.theme)}</span>
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="system">System</SelectItem>
                    <SelectItem value="light">Light</SelectItem>
                    <SelectItem value="dark">Dark</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <p class="text-muted-foreground mt-2 text-xs">
                Theme for this space. System will follow your OS preference
              </p>
            </div>

            <div>
              <h4 class="text-medium mb-3 leading-none font-semibold tracking-tight">
                Notifications
              </h4>
              <div class="flex items-center gap-1.5">
                <Checkbox
                  id="settings.notifications.audio.on_req_finish"
                  bind:checked={appStateRx.space.settings.notifications.audio.on_req_finish}
                />
                <Label for="settings.notifications.audio.on_req_finish" class="cursor-pointer">
                  Play sound when a request finishes
                </Label>
              </div>
              <p class="text-muted-foreground mt-2 text-xs">
                Get audio notification when HTTP requests finishes
              </p>
            </div>
          </div>
        </TabsContent>
      </div>
    </Tabs>
    <DialogFooter>
      <Button
        disabled={spaceSettingsStr === JSON.stringify(appStateRx.space.settings) &&
          userSettingsStr === JSON.stringify(appStateRx.userSettings)}
        onclick={handeSave}
      >
        Save
      </Button>
    </DialogFooter>
  </DialogContent>
{/if}
