import { readFile } from "node:fs/promises";
import adapter from "@sveltejs/adapter-static";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

const pkg = JSON.parse(await readFile("./package.json", "utf-8"));

/** @type {import('@sveltejs/kit').Config} */
const config = {
    preprocess: vitePreprocess(),
    kit: {
        adapter: adapter(),
        alias: {
            "$lib/*": "./src/lib/*",
        },
        version: {
            name: pkg.version,
        },
    },
};

export default config;
