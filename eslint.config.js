import { fileURLToPath } from "node:url";
import eslintJs from "@eslint/js";
import { includeIgnoreFile } from "@eslint/compat";
import eslintTs from "typescript-eslint";
import eslintPluginSvelte from "eslint-plugin-svelte";
import globals from "globals";

import svelteConfig from "./svelte.config.js";

const gitignorePath = fileURLToPath(new URL("./.gitignore", import.meta.url));

/** @type {import('eslint').Linter.Config[]} */
export default eslintTs.config(
    includeIgnoreFile(gitignorePath),
    eslintJs.configs.recommended,
    ...eslintTs.configs.recommended,
    ...eslintPluginSvelte.configs["flat/recommended"],
    {
        languageOptions: {
            globals: {
                ...globals.browser,
                ...globals.node,
            },
        },
    },
    {
        files: ["**/*.svelte", "**/*.svelte.ts"],
        languageOptions: {
            parserOptions: {
                parser: eslintTs.parser,
                projectService: true,
                extraFileExtensions: [".svelte", ".svelte.ts"],
                svelteFeatures: {
                    experimentalGenerics: true,
                },
                svelteConfig,
            },
        },
    },
    {
        ignores: ["src-tauri/", "src/lib/bindings.ts"],
    },
    {
        rules: {
            "@typescript-eslint/no-unused-vars": [
                "warn",
                { varsIgnorePattern: "^_", argsIgnorePattern: "^_" },
            ],
            "@typescript-eslint/no-explicit-any": "off",
        },
    },
);
