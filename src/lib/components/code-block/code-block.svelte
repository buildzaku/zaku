<script lang="ts">
    import { onMount, onDestroy } from "svelte";
    import { Compartment, EditorState } from "@codemirror/state";
    import type { Extension } from "@codemirror/state";
    import {
        drawSelection,
        dropCursor,
        EditorView,
        highlightActiveLine,
        highlightActiveLineGutter,
        highlightSpecialChars,
        keymap,
        lineNumbers,
    } from "@codemirror/view";
    import { json } from "@codemirror/lang-json";
    import { html } from "@codemirror/lang-html";
    import { search, searchKeymap } from "@codemirror/search";
    import {
        bracketMatching,
        defaultHighlightStyle,
        foldGutter,
        foldKeymap,
        syntaxHighlighting,
    } from "@codemirror/language";
    import { defaultKeymap } from "@codemirror/commands";
    import { mode } from "mode-watcher";
    import { darkTheme, lightTheme } from "$lib/components/code-block/themes";

    type Props = { value: string; lang: string; class?: string };

    let { value = $bindable(), lang, class: className }: Props = $props();

    let editorView: EditorView | undefined = $state(undefined);
    let editorElement: HTMLDivElement | undefined = $state(undefined);
    let editorTheme = $state($mode);

    const theme = {
        dark: darkTheme,
        light: lightTheme,
    };

    const themeCompartment = new Compartment();

    const extensions: Extension[] = [
        EditorView.updateListener.of(update => {
            if (update.changes) {
                value = update.state.doc.toString();
            }
        }),
        EditorState.readOnly.of(true),
        EditorView.lineWrapping,
        highlightActiveLine(),
        search({ top: true }),
        lineNumbers(),
        highlightActiveLineGutter(),
        highlightSpecialChars(),
        foldGutter(),
        drawSelection({ cursorBlinkRate: 0 }),
        dropCursor(),
        syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
        bracketMatching(),
        keymap.of([...defaultKeymap, ...searchKeymap, ...foldKeymap]),
    ];

    const createEditor = () => {
        if (lang === "json") {
            extensions.push(json());
        } else if (lang === "html") {
            extensions.push(html());
        }

        const currentTheme = $mode ? theme[$mode] : theme.dark;

        const state = EditorState.create({
            doc: value,
            extensions: [...extensions, themeCompartment.of(currentTheme)],
        });

        editorView = new EditorView({
            state,
            parent: editorElement,
        });
    };

    onMount(() => {
        createEditor();
    });

    $effect(() => {
        if (editorView) {
            const currentValue = editorView.state.doc.toString();
            if (currentValue !== value) {
                const transaction = editorView.state.update({
                    changes: { from: 0, to: currentValue.length, insert: value },
                });
                editorView.dispatch(transaction);
            }
        }
    });

    $effect(() => {
        if (editorView && $mode && $mode !== editorTheme) {
            editorView.dispatch({
                effects: themeCompartment.reconfigure(theme[$mode]),
            });
            editorTheme = $mode;
        }
    });

    onDestroy(() => {
        if (editorView) {
            editorView.destroy();
        }
    });
</script>

<div bind:this={editorElement} class={className}></div>
