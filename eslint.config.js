import { fileURLToPath } from "node:url";
import eslintJs from "@eslint/js";
import { includeIgnoreFile } from "@eslint/compat";
import eslintTs from "typescript-eslint";
import eslintPluginSvelte from "eslint-plugin-svelte";
import globals from "globals";

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
        files: ["**/*.svelte"],
        languageOptions: {
            parserOptions: {
                parser: eslintTs.parser,
            },
        },
    },
    {
        rules: {
            "@typescript-eslint/no-unused-vars": ["warn", { varsIgnorePattern: "^(\\$|_)" }],
            "@typescript-eslint/no-explicit-any": "off",
        },
    },
);
