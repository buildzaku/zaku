import type { CreateCollectionDto } from "$lib/bindings";
import { zakuState } from "$lib/store";
import { safeInvoke } from ".";

export async function createCollection(createCollectionDto: CreateCollectionDto) {
    const createCollectionResult = await safeInvoke("create_collection", {
        create_collection_dto: createCollectionDto,
    });

    if (!createCollectionResult.ok) {
        return false;
    }

    await zakuState.synchronize();

    return true;
}
