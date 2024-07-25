import { EditorView } from "@codemirror/view";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags } from "@lezer/highlight";

export const light = EditorView.theme(
    {
        "&": {
            color: "#383a42",
            backgroundColor: "transparent",
        },
        ".cm-content": {
            caretColor: "#000",
            fontFamily: "monospace",
        },
        "&.cm-focused .cm-cursor": {
            borderLeftColor: "#000",
        },
        "&.cm-focused .cm-selectionBackground, ::selection": {
            backgroundColor: "#add6ff",
        },
        ".cm-selectionMatch": {
            backgroundColor: "#a8ac94",
        },
        ".cm-line": {
            backgroundColor: "transparent",
        },
        ".cm-activeLine": {
            boxShadow: "inset 0px 0px 400px 110px rgba(0, 0, 0, 0.10)",
        },
        ".cm-gutters": {
            backgroundColor: "#e9ebef",
            color: "#237893",
            border: "none",
            cursor: "default",
        },
        ".cm-activeLineGutter": {
            color: "#0b216f",
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
        color: "#0000ff",
    },
    { tag: [tags.moduleKeyword, tags.controlKeyword], color: "#af00db" },
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
        color: "#0070c1",
    },
    { tag: tags.heading, fontWeight: "bold", color: "#0070c1" },
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
        color: "#267f99",
    },
    { tag: [tags.function(tags.variableName), tags.function(tags.propertyName)], color: "#795e26" },
    { tag: [tags.number], color: "#098658" },
    {
        tag: [tags.operator, tags.punctuation, tags.separator, tags.url, tags.escape, tags.regexp],
        color: "#383a42",
    },
    { tag: [tags.regexp], color: "#af00db" },
    {
        tag: [tags.special(tags.string), tags.processingInstruction, tags.string, tags.inserted],
        color: "#a31515",
    },
    { tag: [tags.angleBracket], color: "#383a42" },
    { tag: tags.strong, fontWeight: "bold" },
    { tag: tags.emphasis, fontStyle: "italic" },
    { tag: tags.strikethrough, textDecoration: "line-through" },
    { tag: [tags.meta, tags.comment], color: "#008000" },
    { tag: tags.link, color: "#4078f2", textDecoration: "underline" },
    { tag: tags.invalid, color: "#e45649" },
]);

export const lightTheme = [light, syntaxHighlighting(highlights)];
