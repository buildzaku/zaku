import eslintJs from "@eslint/js";
import eslintTs from "typescript-eslint";
import eslintPluginSvelte from "eslint-plugin-svelte";
import globals from "globals";

/** @type {import('eslint').Linter.Config[]} */
export default [
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
                parser: ts.parser,
            },
        },
    },
    {
        ignores: ["build/", ".svelte-kit/", "dist/", "src-tauri/"],
    },
    {
        rules: {
            "@typescript-eslint/no-unused-vars": [
                "warn",
                { varsIgnorePattern: "^\\$\\$(Props|Events|Slots)$" },
            ],
        },
    },
];
