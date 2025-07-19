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
    import { search, searchKeymap } from "@codemirror/search";
    import {
        bracketMatching,
        defaultHighlightStyle,
        foldGutter,
        foldKeymap,
        LanguageSupport,
        syntaxHighlighting,
    } from "@codemirror/language";
    import { defaultKeymap } from "@codemirror/commands";
    import { mode } from "mode-watcher";
    import type { SystemModeValue } from "mode-watcher";
    import { darkTheme, lightTheme } from "$lib/components/code-block/themes";

    type Props = {
        value?: string;
        language: LanguageSupport | null;
        readOnly?: boolean;
        class?: string;
    };

    let {
        value = $bindable(),
        language = $bindable(),
        readOnly = false,
        class: className,
    }: Props = $props();

    let editorView: EditorView | undefined = $state(undefined);
    let editorElement: HTMLDivElement | undefined = $state(undefined);
    let currentTheme: SystemModeValue = $state(mode.current);

    const theme = {
        dark: darkTheme,
        light: lightTheme,
    };

    const themeCompartment = new Compartment();
    const languageCompartment = new Compartment();

    const extensions: Extension[] = [
        EditorView.updateListener.of(update => {
            if (update.docChanged) {
                value = update.state.doc.toString();
            }
        }),
        EditorState.readOnly.of(readOnly),
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
        if (language) {
            extensions.push(languageCompartment.of(language));
        }

        const selectedTheme = mode.current ? theme[mode.current] : theme.dark;
        const state = EditorState.create({
            doc: value,
            extensions: [...extensions, themeCompartment.of(selectedTheme)],
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
            if (value !== editorView.state.doc.toString()) {
                editorView.dispatch({
                    changes: {
                        from: 0,
                        to: editorView.state.doc.length,
                        insert: value,
                    },
                });
            }

            if (language) {
                editorView.dispatch({
                    effects: languageCompartment.reconfigure(language),
                });
            } else {
                editorView.dispatch({
                    effects: languageCompartment.reconfigure([]),
                });
            }

            if (mode.current && mode.current !== currentTheme) {
                editorView.dispatch({
                    effects: themeCompartment.reconfigure(theme[mode.current]),
                });
                currentTheme = mode.current;
            }
        }
    });

    onDestroy(() => {
        if (editorView) {
            editorView.destroy();
        }
    });
</script>

<div bind:this={editorElement} class={className}></div>
