import type { CreateCollectionDto } from "$lib/bindings";
import { zakuState } from "$lib/store";
import { safeInvoke } from ".";

export async function createCollection(createCollectionDto: CreateCollectionDto) {
    safeInvoke("create_collection", { create_collection_dto: createCollectionDto });

    await zakuState.synchronize();
}
