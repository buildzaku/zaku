import { EditorView } from "@codemirror/view";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags } from "@lezer/highlight";

export const light = EditorView.theme(
    {
        "&": {
            color: "#2a2d3a",
            backgroundColor: "transparent",
        },
        ".cm-content": {
            caretColor: "#2a2d3a",
            fontFamily: "monospace",
        },
        "&.cm-focused .cm-cursor": {
            borderLeftColor: "#2a2d3a",
        },
        "&.cm-focused .cm-selectionBackground, ::selection": {
            backgroundColor: "#c5d9f1",
        },
        ".cm-selectionMatch": {
            backgroundColor: "#dae8f7",
        },
        ".cm-line": {
            backgroundColor: "transparent",
        },
        ".cm-activeLine": {
            backgroundColor: "#f8f8f9",
        },
        ".cm-gutters": {
            backgroundColor: "#ededef",
            color: "#6b6d7a",
            border: "none",
            cursor: "default",
        },
        ".cm-activeLineGutter": {
            color: "#2a2d3a",
            backgroundColor: "transparent",
        },
    },
    { dark: false },
);

const highlights = HighlightStyle.define([
    {
        tag: [
            tags.keyword,
            tags.operatorKeyword,
            tags.modifier,
            tags.color,
            tags.constant(tags.name),
            tags.standard(tags.name),
            tags.standard(tags.tagName),
            tags.special(tags.brace),
            tags.atom,
            tags.bool,
            tags.special(tags.variableName),
        ],
        color: "#0052cc",
    },
    { tag: [tags.moduleKeyword, tags.controlKeyword], color: "#9333ea" },
    {
        tag: [
            tags.name,
            tags.deleted,
            tags.character,
            tags.macroName,
            tags.propertyName,
            tags.variableName,
            tags.labelName,
            tags.definition(tags.name),
        ],
        color: "#1c3998",
    },
    { tag: tags.heading, fontWeight: "bold", color: "#1c3998" },
    {
        tag: [
            tags.typeName,
            tags.className,
            tags.tagName,
            tags.number,
            tags.changed,
            tags.annotation,
            tags.self,
            tags.namespace,
        ],
        color: "#0891b2",
    },
    {
        tag: [tags.function(tags.variableName), tags.function(tags.propertyName)],
        color: "#d97706",
    },
    { tag: [tags.number], color: "#16a34a" },
    {
        tag: [tags.operator, tags.punctuation, tags.separator, tags.url, tags.escape, tags.regexp],
        color: "#2a2d3a",
    },
    { tag: [tags.regexp], color: "#c026d3" },
    {
        tag: [tags.special(tags.string), tags.processingInstruction, tags.string, tags.inserted],
        color: "#c8430a",
    },
    { tag: [tags.angleBracket], color: "#6b6d7a" },
    { tag: tags.strong, fontWeight: "bold" },
    { tag: tags.emphasis, fontStyle: "italic" },
    { tag: tags.strikethrough, textDecoration: "line-through" },
    { tag: [tags.meta, tags.comment], color: "#16a34a" },
    { tag: tags.link, color: "#0052cc", textDecoration: "underline" },
    { tag: tags.invalid, color: "#dc2626" },
]);

export const lightTheme = [light, syntaxHighlighting(highlights)];
